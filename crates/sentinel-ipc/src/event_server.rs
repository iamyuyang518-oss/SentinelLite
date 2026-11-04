//! Read-only event-delivery server over a restricted local named pipe.
//!
//! This is a separate endpoint from the status pipe. Event batches are bounded
//! and a slow or malicious reader can only consume the 2-second I/O timeout of
//! its own connection; it cannot block ETW collection or health queries.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Pipes::DisconnectNamedPipe;

use crate::events::{
    EventBatch, EventResponse, MAX_EVENT_REQUEST_BYTES, decode_event_request, encode_event_batch,
    encode_event_response,
};
use crate::protocol::ProtocolErrorCode;
use crate::server::{
    AuthError, ServerError, authenticate_client, create_event, create_pipe,
    is_transient_connect_error, map_io, validate_pipe_name,
};
use crate::win::{self, OwnedHandle};

const ACCEPT_TIMEOUT: Duration = Duration::from_millis(250);
const EVENT_PIPE_BUFFER_BYTES: u32 = 64 * 1024;

pub trait EventProvider: Send + Sync + 'static {
    /// Return a bounded batch of records after `after`. The provider owns
    /// cursor-expiry and session semantics and must never block for long.
    fn batch(&self, after: u64, limit: u16) -> Result<EventBatch, String>;

    /// Called after a batch was written successfully. The sensor uses this to
    /// know when the final `SessionEnd` record reached a client.
    fn acknowledge(&self, _cursor: u64) {}
}

#[derive(Debug, Clone)]
pub struct EventServerConfig {
    pub pipe_name: String,
    pub io_timeout: Duration,
    require_medium_label: bool,
    startup_error: Option<ServerError>,
}

impl EventServerConfig {
    pub fn new(pipe_name: impl Into<String>) -> Self {
        Self {
            pipe_name: pipe_name.into(),
            io_timeout: Duration::from_secs(2),
            require_medium_label: true,
            startup_error: None,
        }
    }

    /// Test-only configuration for non-elevated test processes.
    #[doc(hidden)]
    pub fn for_testing(pipe_name: impl Into<String>) -> Self {
        Self {
            pipe_name: pipe_name.into(),
            io_timeout: Duration::from_secs(2),
            require_medium_label: false,
            startup_error: None,
        }
    }

    pub fn with_io_timeout(mut self, io_timeout: Duration) -> Self {
        self.io_timeout = io_timeout;
        self
    }

    #[doc(hidden)]
    pub fn with_startup_error_for_testing(mut self, error: ServerError) -> Self {
        self.startup_error = Some(error);
        self
    }
}

#[derive(Debug)]
pub struct EventServer {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), ServerError>>>,
}

impl EventServer {
    pub fn start(
        config: EventServerConfig,
        provider: Arc<dyn EventProvider>,
    ) -> Result<Self, ServerError> {
        validate_pipe_name(&config.pipe_name)?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), ServerError>>();
        let thread = thread::Builder::new()
            .name("sentinel-ipc-events".to_owned())
            .spawn(move || run(config, provider, thread_shutdown, ready_tx))
            .map_err(|error| {
                ServerError::Config(format!("failed to spawn event IPC server: {error}"))
            })?;

        match ready_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(Self {
                shutdown,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                shutdown.store(true, Ordering::SeqCst);
                match thread.join() {
                    Ok(Err(error)) => Err(error),
                    Ok(Ok(())) => Err(ServerError::Config(
                        "event IPC server did not become ready in time".to_owned(),
                    )),
                    Err(_) => Err(ServerError::ThreadPanic),
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => match thread.join() {
                Ok(Err(error)) => Err(error),
                Ok(Ok(())) => Err(ServerError::ThreadPanic),
                Err(_) => Err(ServerError::ThreadPanic),
            },
        }
    }

    pub fn stop(&mut self) -> Result<(), ServerError> {
        self.shutdown.store(true, Ordering::SeqCst);
        let Some(thread) = self.thread.take() else {
            return Ok(());
        };
        match thread.join() {
            Ok(result) => result,
            Err(_) => Err(ServerError::ThreadPanic),
        }
    }
}

impl Drop for EventServer {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn run(
    config: EventServerConfig,
    provider: Arc<dyn EventProvider>,
    shutdown: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(), ServerError>>,
) -> Result<(), ServerError> {
    if let Some(error) = config.startup_error {
        return Err(error);
    }
    let user_sid = win::current_process_user_sid_string().map_err(|error| {
        ServerError::Security(format!("could not read current user SID: {error}"))
    })?;
    let pipe = OwnedHandle(create_pipe(
        &config.pipe_name,
        &user_sid,
        config.require_medium_label,
        EVENT_PIPE_BUFFER_BYTES,
    )?);
    let event = create_event()?;
    let pipe_handle = pipe.0;
    let _pipe = pipe;
    let _event = OwnedHandle(event);

    if ready.send(Ok(())).is_err() {
        return Ok(());
    }

    loop {
        if shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }
        match win::connect(pipe_handle, event, ACCEPT_TIMEOUT) {
            Ok(()) => {
                handle_client(
                    pipe_handle,
                    event,
                    provider.as_ref(),
                    &user_sid,
                    config.io_timeout,
                )?;
                // SAFETY: the handle is a connected named-pipe instance.
                unsafe {
                    DisconnectNamedPipe(pipe_handle);
                }
            }
            Err(error) if error.timeout => continue,
            Err(error) if is_transient_connect_error(error.code) => {
                // SAFETY: the handle is a named-pipe instance.
                unsafe {
                    DisconnectNamedPipe(pipe_handle);
                }
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(error) => return Err(map_io(error)),
        }
    }
}

fn handle_client(
    pipe: HANDLE,
    event: HANDLE,
    provider: &dyn EventProvider,
    server_sid: &str,
    io_timeout: Duration,
) -> Result<(), ServerError> {
    match authenticate_client(pipe, server_sid) {
        Ok(()) => {}
        Err(AuthError::Rejected(message)) => {
            write_failure(
                pipe,
                event,
                io_timeout,
                ProtocolErrorCode::Unauthorized,
                message,
            );
            return Ok(());
        }
        Err(AuthError::Fatal(error)) => return Err(error),
    }

    let mut request_buffer = vec![0u8; MAX_EVENT_REQUEST_BYTES + 1];
    let bytes = match win::read(pipe, event, &mut request_buffer, io_timeout) {
        Ok(bytes) => bytes,
        Err(error) if error.timeout => {
            write_failure(
                pipe,
                event,
                io_timeout,
                ProtocolErrorCode::Timeout,
                error.message,
            );
            return Ok(());
        }
        Err(error) if error.more_data => {
            write_failure(
                pipe,
                event,
                io_timeout,
                ProtocolErrorCode::RequestTooLarge,
                error.message,
            );
            return Ok(());
        }
        Err(_) => return Ok(()),
    };

    if bytes > MAX_EVENT_REQUEST_BYTES {
        write_failure(
            pipe,
            event,
            io_timeout,
            ProtocolErrorCode::RequestTooLarge,
            "event request message exceeds limit",
        );
        return Ok(());
    }

    let request = match decode_event_request(&request_buffer[..bytes]) {
        Ok(request) => request,
        Err(error) => {
            write_failure(pipe, event, io_timeout, error.code, error.message);
            return Ok(());
        }
    };

    let batch = match provider.batch(request.after, request.limit) {
        Ok(batch) => batch,
        Err(message) => {
            write_failure(
                pipe,
                event,
                io_timeout,
                ProtocolErrorCode::Internal,
                message,
            );
            return Ok(());
        }
    };
    let acknowledged = batch.next_cursor;
    let response = match encode_event_batch(batch) {
        Ok(bytes) => bytes,
        Err(error) => {
            write_failure(pipe, event, io_timeout, error.code, error.message);
            return Ok(());
        }
    };
    if win::write(pipe, event, &response, io_timeout).is_ok() {
        provider.acknowledge(acknowledged);
    }
    Ok(())
}

fn write_failure(
    pipe: HANDLE,
    event: HANDLE,
    io_timeout: Duration,
    code: ProtocolErrorCode,
    message: impl Into<String>,
) {
    let Ok(bytes) = encode_event_response(&EventResponse::failure(code, message)) else {
        return;
    };
    let _ = win::write(pipe, event, &bytes, io_timeout);
}
