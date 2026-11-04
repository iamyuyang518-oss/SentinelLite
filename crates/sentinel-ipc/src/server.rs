//! Read-only status-query server over a restricted local named pipe.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_NO_DATA, ERROR_PIPE_NOT_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, RevertToSelf, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, DisconnectNamedPipe, ImpersonateNamedPipeClient, PIPE_READMODE_MESSAGE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::CreateEventW;

use crate::protocol::{
    MAX_REQUEST_BYTES, ProtocolErrorCode, SensorStatus, StatusResponse, decode_request,
    encode_response,
};
use crate::win::{self, IoError, OwnedHandle, PrivilegeRestoreOutcome};

const ACCEPT_TIMEOUT: Duration = Duration::from_millis(250);
const PIPE_BUFFER_BYTES: u32 = 4096;

pub trait StatusProvider: Send + Sync + 'static {
    fn status(&self) -> Result<SensorStatus, String>;
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub pipe_name: String,
    pub io_timeout: Duration,
    require_medium_label: bool,
    startup_error: Option<ServerError>,
}

impl ServerConfig {
    pub fn new(pipe_name: impl Into<String>) -> Self {
        Self {
            pipe_name: pipe_name.into(),
            io_timeout: Duration::from_secs(2),
            require_medium_label: true,
            startup_error: None,
        }
    }

    /// Test-only configuration for non-elevated test processes. The
    /// production sensor uses `new`, which requires the Medium label.
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

    /// Test-only startup error injection. This exercises the same
    /// `Server::start` error boundary used by the sensor without requiring
    /// Windows privilege failures.
    #[doc(hidden)]
    pub fn with_startup_error_for_testing(mut self, error: ServerError) -> Self {
        self.startup_error = Some(error);
        self
    }
}

#[derive(Debug, Clone)]
pub enum ServerError {
    Config(String),
    Security(String),
    CreatePipe {
        code: u32,
        message: String,
        privilege_restore_failed: bool,
    },
    PrivilegeRestore(String),
    ThreadPanic,
    Io {
        code: u32,
        message: String,
    },
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message) => write!(f, "IPC server configuration error: {message}"),
            Self::Security(message) => write!(f, "IPC server security error: {message}"),
            Self::CreatePipe {
                code,
                message,
                privilege_restore_failed,
            } => {
                write!(
                    f,
                    "IPC pipe creation failed ({code}): {message}{}",
                    if *privilege_restore_failed {
                        " (SeSecurityPrivilege restore also failed)"
                    } else {
                        ""
                    }
                )
            }
            Self::PrivilegeRestore(message) => {
                write!(f, "IPC privilege restore failed: {message}")
            }
            Self::ThreadPanic => f.write_str("IPC server thread panicked"),
            Self::Io { code, message } => write!(f, "IPC server I/O failed ({code}): {message}"),
        }
    }
}

impl ServerError {
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::PrivilegeRestore(_)
                | Self::CreatePipe {
                    privilege_restore_failed: true,
                    ..
                }
        )
    }
}

impl std::error::Error for ServerError {}

pub struct Server {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<(), ServerError>>>,
}

impl Server {
    pub fn start(
        config: ServerConfig,
        provider: Arc<dyn StatusProvider>,
    ) -> Result<Self, ServerError> {
        validate_pipe_name(&config.pipe_name)?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), ServerError>>();
        let thread = thread::Builder::new()
            .name("sentinel-ipc-server".to_owned())
            .spawn(move || run(config, provider, thread_shutdown, ready_tx))
            .map_err(|error| ServerError::Config(format!("failed to spawn IPC server: {error}")))?;

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
                        "IPC server did not become ready in time".to_owned(),
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

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub(crate) fn validate_pipe_name(pipe_name: &str) -> Result<(), ServerError> {
    if !pipe_name.starts_with(r"\\.\pipe\") {
        return Err(ServerError::Config(
            "pipe name must start with \\\\.\\pipe\\".to_owned(),
        ));
    }
    if pipe_name.encode_utf16().count() > 256 {
        return Err(ServerError::Config(
            "pipe name must be at most 256 UTF-16 units".to_owned(),
        ));
    }
    Ok(())
}

fn run(
    config: ServerConfig,
    provider: Arc<dyn StatusProvider>,
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
        PIPE_BUFFER_BYTES,
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
                // A client can connect and disappear between instances. Reset
                // the instance and keep the status service alive.
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

pub(crate) fn is_transient_connect_error(code: u32) -> bool {
    matches!(
        code,
        ERROR_NO_DATA | ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED
    )
}

pub(crate) fn create_pipe(
    pipe_name: &str,
    user_sid: &str,
    require_medium_label: bool,
    buffer_bytes: u32,
) -> Result<HANDLE, ServerError> {
    let mut privilege = if require_medium_label {
        Some(win::enable_security_privilege().map_err(|error| {
            ServerError::Security(format!(
                "could not enable SeSecurityPrivilege for pipe label: {error}"
            ))
        })?)
    } else {
        None
    };

    let sddl = pipe_sddl(user_sid, require_medium_label);
    let sddl_wide = win::wide(&sddl);
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: the SDDL string is NUL-terminated and descriptor is writable.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl_wide.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        let error = win::last_error_code();
        let restore_outcome = privilege
            .as_mut()
            .map(|guard| guard.restore())
            .unwrap_or(PrivilegeRestoreOutcome::Restored);
        if let PrivilegeRestoreOutcome::Failed(restore_error) = restore_outcome {
            return Err(ServerError::PrivilegeRestore(format!(
                "invalid pipe security descriptor ({error}) and privilege restore failed: \
                 {restore_error}"
            )));
        }
        return Err(ServerError::Security(format!(
            "invalid pipe security descriptor ({error}): {}",
            std::io::Error::from_raw_os_error(error as i32)
        )));
    }

    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let name_wide = win::wide(pipe_name);
    // SAFETY: name_wide and attributes are live for the call; the kernel
    // copies the security descriptor.
    let handle = unsafe {
        CreateNamedPipeW(
            name_wide.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            buffer_bytes,
            buffer_bytes,
            0,
            &attributes,
        )
    };
    // Capture LastError before LocalFree / privilege restoration, either of
    // which can change the thread's last error.
    let create_error = if handle == INVALID_HANDLE_VALUE {
        win::last_error_code()
    } else {
        0
    };
    // SAFETY: descriptor was allocated by ConvertStringSecurityDescriptor...
    // and is no longer needed after CreateNamedPipeW succeeds or fails.
    unsafe {
        LocalFree(descriptor);
    }
    let restore_outcome = privilege
        .as_mut()
        .map(|guard| guard.restore())
        .unwrap_or(PrivilegeRestoreOutcome::Restored);
    finalize_pipe_creation(handle, create_error, restore_outcome, |handle| {
        // SAFETY: handle was created successfully and is owned here.
        unsafe {
            CloseHandle(handle);
        }
    })
}

fn finalize_pipe_creation<F>(
    handle: HANDLE,
    create_error: u32,
    restore_outcome: PrivilegeRestoreOutcome,
    close: F,
) -> Result<HANDLE, ServerError>
where
    F: FnOnce(HANDLE),
{
    if handle == INVALID_HANDLE_VALUE {
        return Err(pipe_create_failure(create_error, &restore_outcome));
    }
    if let PrivilegeRestoreOutcome::Failed(error) = restore_outcome {
        // Do not leave a live pipe with an uncertain privilege state.
        close(handle);
        return Err(ServerError::PrivilegeRestore(format!(
            "could not restore SeSecurityPrivilege after pipe creation: {error}"
        )));
    }
    Ok(handle)
}

fn pipe_create_failure(code: u32, restore: &PrivilegeRestoreOutcome) -> ServerError {
    match restore {
        PrivilegeRestoreOutcome::Failed(error) => ServerError::CreatePipe {
            code,
            message: format!(
                "{}; SeSecurityPrivilege restore failed: {error}",
                std::io::Error::from_raw_os_error(code as i32)
            ),
            privilege_restore_failed: true,
        },
        PrivilegeRestoreOutcome::Restored | PrivilegeRestoreOutcome::RestoredAfterRetry => {
            ServerError::CreatePipe {
                code,
                message: std::io::Error::from_raw_os_error(code as i32).to_string(),
                privilege_restore_failed: false,
            }
        }
    }
}

fn pipe_sddl(user_sid: &str, require_medium_label: bool) -> String {
    let dacl = format!("D:(A;;GRGW;;;{user_sid})(A;;GA;;;SY)(A;;GA;;;BA)");
    if require_medium_label {
        format!("{dacl}S:(ML;;NW;;;S-1-16-8192)")
    } else {
        dacl
    }
}

pub(crate) fn create_event() -> Result<HANDLE, ServerError> {
    // SAFETY: no security attributes, manual-reset event, unnamed.
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event.is_null() {
        return Err(map_io(IoError::last_error()));
    }
    Ok(event)
}

fn handle_client(
    pipe: HANDLE,
    event: HANDLE,
    provider: &dyn StatusProvider,
    server_sid: &str,
    io_timeout: Duration,
) -> Result<(), ServerError> {
    match authenticate_client(pipe, server_sid) {
        Ok(()) => {}
        Err(AuthError::Rejected(message)) => {
            let response = StatusResponse::failure(ProtocolErrorCode::Unauthorized, message);
            write_response(pipe, event, &response, io_timeout);
            return Ok(());
        }
        Err(AuthError::Fatal(error)) => return Err(error),
    }

    let mut request_buffer = vec![0u8; MAX_REQUEST_BYTES + 1];
    let bytes = match win::read(pipe, event, &mut request_buffer, io_timeout) {
        Ok(bytes) => bytes,
        Err(error) if error.timeout => {
            let response = StatusResponse::failure(ProtocolErrorCode::Timeout, error.message);
            write_response(pipe, event, &response, io_timeout);
            return Ok(());
        }
        Err(error) if error.more_data => {
            let response =
                StatusResponse::failure(ProtocolErrorCode::RequestTooLarge, error.message);
            write_response(pipe, event, &response, io_timeout);
            return Ok(());
        }
        Err(_) => return Ok(()),
    };

    let response = if bytes > MAX_REQUEST_BYTES {
        StatusResponse::failure(
            ProtocolErrorCode::RequestTooLarge,
            "request message exceeds limit",
        )
    } else {
        match decode_request(&request_buffer[..bytes]) {
            Ok(_) => match provider.status() {
                Ok(status) => StatusResponse::success(status),
                Err(message) => StatusResponse::failure(ProtocolErrorCode::Internal, message),
            },
            Err(error) => StatusResponse::failure(error.code, error.message),
        }
    };
    write_response(pipe, event, &response, io_timeout);
    Ok(())
}

pub(crate) enum AuthError {
    Rejected(String),
    Fatal(ServerError),
}

pub(crate) fn authenticate_client(pipe: HANDLE, server_sid: &str) -> Result<(), AuthError> {
    // SAFETY: pipe is a connected named-pipe handle.
    if unsafe { ImpersonateNamedPipeClient(pipe) } == 0 {
        let code = win::last_error_code();
        return Err(AuthError::Rejected(format!(
            "client impersonation failed ({code}): {}",
            std::io::Error::from_raw_os_error(code as i32)
        )));
    }

    let client_sid = win::current_thread_user_sid_string();
    // SAFETY: this thread is impersonating the client and must always revert.
    let reverted = unsafe { RevertToSelf() };
    if reverted == 0 {
        return Err(AuthError::Fatal(ServerError::Security(
            "failed to revert client impersonation".to_owned(),
        )));
    }
    let client_sid = client_sid
        .map_err(|error| AuthError::Rejected(format!("client token user unavailable: {error}")))?;
    if client_sid != server_sid {
        return Err(AuthError::Rejected(
            "client user does not match sensor user".to_owned(),
        ));
    }
    Ok(())
}

fn write_response(pipe: HANDLE, event: HANDLE, response: &StatusResponse, io_timeout: Duration) {
    let bytes = match encode_response(response) {
        Ok(bytes) => bytes,
        Err(error) => {
            let fallback =
                StatusResponse::failure(ProtocolErrorCode::ResponseTooLarge, error.message);
            match encode_response(&fallback) {
                Ok(bytes) => bytes,
                Err(_) => return,
            }
        }
    };
    let _ = win::write(pipe, event, &bytes, io_timeout);
}

pub(crate) fn map_io(error: IoError) -> ServerError {
    ServerError::Io {
        code: error.code,
        message: error.message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_pipe_sddl_has_medium_mandatory_label() {
        let sddl = pipe_sddl("S-1-5-21-1-2-3-1001", true);
        assert!(sddl.contains("D:(A;;GRGW;;;S-1-5-21-1-2-3-1001)"));
        assert!(sddl.ends_with("S:(ML;;NW;;;S-1-16-8192)"));
    }

    #[test]
    fn test_pipe_sddl_omits_the_label_for_non_elevated_tests() {
        let sddl = pipe_sddl("S-1-5-21-1-2-3-1001", false);
        assert!(!sddl.contains("S:(ML;"));
    }

    #[test]
    fn create_pipe_failure_preserves_code_when_restore_fails() {
        let restore = PrivilegeRestoreOutcome::Failed(IoError {
            code: 1300,
            message: "restore changed last error".to_owned(),
            timeout: false,
            more_data: false,
        });
        let error = pipe_create_failure(5, &restore);
        match error {
            ServerError::CreatePipe {
                code,
                privilege_restore_failed,
                message,
            } => {
                assert_eq!(code, 5);
                assert!(privilege_restore_failed);
                assert!(message.contains("1300"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(pipe_create_failure(5, &restore).is_fatal());
    }

    #[test]
    fn finalize_pipe_creation_preserves_create_error_when_restore_fails() {
        let result = finalize_pipe_creation(
            INVALID_HANDLE_VALUE,
            5,
            PrivilegeRestoreOutcome::Failed(IoError {
                code: 1300,
                message: "restore changed last error".to_owned(),
                timeout: false,
                more_data: false,
            }),
            |_| panic!("no handle should be closed for an invalid handle"),
        );
        match result {
            Err(ServerError::CreatePipe {
                code,
                privilege_restore_failed,
                ..
            }) => {
                assert_eq!(code, 5);
                assert!(privilege_restore_failed);
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn create_pipe_failure_keeps_original_code_when_restore_succeeds() {
        let error = pipe_create_failure(5, &PrivilegeRestoreOutcome::Restored);
        match error {
            ServerError::CreatePipe {
                code,
                privilege_restore_failed,
                ..
            } => {
                assert_eq!(code, 5);
                assert!(!privilege_restore_failed);
            }
            other => panic!("unexpected error: {other:?}"),
        }
        assert!(!pipe_create_failure(5, &PrivilegeRestoreOutcome::Restored).is_fatal());
    }

    #[test]
    fn restore_failure_after_pipe_creation_closes_the_handle() {
        let fake_handle = 1usize as HANDLE;
        let mut closed = false;
        let result = finalize_pipe_creation(
            fake_handle,
            0,
            PrivilegeRestoreOutcome::Failed(IoError {
                code: 1300,
                message: "persistent restore failure".to_owned(),
                timeout: false,
                more_data: false,
            }),
            |handle| {
                assert_eq!(handle, fake_handle);
                closed = true;
            },
        );
        assert!(matches!(result, Err(ServerError::PrivilegeRestore(_))));
        assert!(closed, "the created pipe handle must be closed");
    }

    #[test]
    fn privilege_restore_errors_are_fatal() {
        assert!(ServerError::PrivilegeRestore("restore failed".to_owned()).is_fatal());
        assert!(!ServerError::Security("label setup failed".to_owned()).is_fatal());
    }

    #[test]
    fn transient_connect_errors_are_classified() {
        assert!(is_transient_connect_error(ERROR_NO_DATA));
        assert!(is_transient_connect_error(ERROR_BROKEN_PIPE));
        assert!(is_transient_connect_error(ERROR_PIPE_NOT_CONNECTED));
        assert!(!is_transient_connect_error(5));
    }

    #[test]
    fn production_pipe_sddl_parses_with_a_sacl() {
        let sddl = pipe_sddl("S-1-5-21-1-2-3-1001", true);
        let wide = win::wide(&sddl);
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: the SDDL is a valid NUL-terminated string and descriptor
        // is writable.
        assert_ne!(
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    wide.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            },
            0
        );
        let mut sacl_present = 0;
        let mut sacl = std::ptr::null_mut();
        let mut sacl_defaulted = 0;
        // SAFETY: descriptor was returned by the conversion above.
        assert_ne!(
            unsafe {
                windows_sys::Win32::Security::GetSecurityDescriptorSacl(
                    descriptor,
                    &mut sacl_present,
                    &mut sacl,
                    &mut sacl_defaulted,
                )
            },
            0
        );
        assert_ne!(sacl_present, 0);
        assert!(!sacl.is_null());
        // SAFETY: descriptor is owned by this test.
        unsafe {
            LocalFree(descriptor);
        }
    }
}
