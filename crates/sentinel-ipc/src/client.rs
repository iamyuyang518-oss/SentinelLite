//! Read-only named-pipe client for `SentinelLiteSensor.exe`.

use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY,
    ERROR_PIPE_NOT_CONNECTED, ERROR_SEM_TIMEOUT, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
};
use windows_sys::Win32::System::Pipes::{
    PIPE_READMODE_MESSAGE, SetNamedPipeHandleState, WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::CreateEventW;

use crate::events::{
    EventBatch, EventRequest, MAX_EVENT_RESPONSE_BYTES, decode_event_response, encode_event_request,
};
use crate::protocol::{
    MAX_RESPONSE_BYTES, PROTOCOL_VERSION, ProtocolError, ProtocolErrorCode, SensorStatus,
    StatusRequest, decode_response, encode_request,
};
use crate::win::{self, IoError};

pub const DEFAULT_CLIENT_TIMEOUT: Duration = Duration::from_secs(2);
pub const SENSOR_EXE_NAME: &str = "SentinelLiteSensor.exe";

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub timeout: Duration,
    pub verify_server: bool,
    pub expected_server_exe: Option<String>,
    pub require_elevation: bool,
}

impl ClientOptions {
    /// Production policy: verify the pipe server image and elevation.
    pub fn secure() -> Self {
        Self {
            timeout: DEFAULT_CLIENT_TIMEOUT,
            verify_server: true,
            expected_server_exe: Some(SENSOR_EXE_NAME.to_owned()),
            require_elevation: true,
        }
    }

    /// Test-only policy. Production callers must use [`Self::secure`].
    #[doc(hidden)]
    pub fn for_testing() -> Self {
        Self {
            timeout: DEFAULT_CLIENT_TIMEOUT,
            verify_server: false,
            expected_server_exe: None,
            require_elevation: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientError {
    NotRunning,
    Timeout,
    Unavailable(String),
    ConnectionLost(String),
    Protocol(ProtocolError),
    Remote(ProtocolError),
    Io { code: u32, message: String },
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRunning => f.write_str(
                "sensor status endpoint does not exist; the sensor may be stopped or its IPC channel may be unavailable",
            ),
            Self::Timeout => f.write_str("sensor IPC timed out"),
            Self::Unavailable(message) => write!(f, "sensor is unavailable: {message}"),
            Self::ConnectionLost(message) => {
                write!(f, "sensor IPC connection was lost: {message}")
            }
            Self::Protocol(error) => write!(f, "sensor protocol error: {}", error.message),
            Self::Remote(error) => write!(f, "sensor rejected status query: {}", error.message),
            Self::Io { code, message } => write!(f, "sensor IPC failed ({code}): {message}"),
        }
    }
}

impl std::error::Error for ClientError {}

pub fn query_status(pipe_name: &str, options: &ClientOptions) -> Result<SensorStatus, ClientError> {
    if pipe_name.is_empty() {
        return Err(ClientError::Unavailable("pipe name is empty".to_owned()));
    }
    let deadline = Instant::now() + options.timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ClientError::Timeout);
        }
        match query_once(pipe_name, options, remaining) {
            Err(ClientError::ConnectionLost(_)) if Instant::now() < deadline => {
                // The server may be between DisconnectNamedPipe and the next
                // ConnectNamedPipe. The status query is idempotent, so retry.
                std::thread::sleep(Duration::from_millis(10));
            }
            result => return result,
        }
    }
}

/// Read one bounded batch of normalized events after `after`.
///
/// The operation is idempotent while the sensor's delivery window retains the
/// records. A lost connection is retried within the client timeout because the
/// cursor makes the replay safe.
pub fn query_events(
    pipe_name: &str,
    options: &ClientOptions,
    after: u64,
    limit: u16,
) -> Result<EventBatch, ClientError> {
    if pipe_name.is_empty() {
        return Err(ClientError::Unavailable("pipe name is empty".to_owned()));
    }
    let deadline = Instant::now() + options.timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ClientError::Timeout);
        }
        match query_events_once(pipe_name, options, after, limit, remaining) {
            Err(ClientError::ConnectionLost(_)) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => return result,
        }
    }
}

fn query_events_once(
    pipe_name: &str,
    options: &ClientOptions,
    after: u64,
    limit: u16,
    timeout: Duration,
) -> Result<EventBatch, ClientError> {
    let handle = connect(pipe_name, timeout)?;
    let _handle = win::OwnedHandle(handle);
    let event = create_event()?;
    let _event = win::OwnedHandle(event);

    if options.verify_server {
        verify_server(handle, options)?;
    }

    let request = EventRequest::new(after, limit);
    let request_bytes = encode_event_request(&request).map_err(ClientError::Protocol)?;
    win::write(handle, event, &request_bytes, timeout).map_err(map_io)?;

    let mut response_buffer = vec![0u8; MAX_EVENT_RESPONSE_BYTES + 1];
    let bytes = win::read(handle, event, &mut response_buffer, timeout).map_err(map_io)?;
    if bytes > MAX_EVENT_RESPONSE_BYTES {
        return Err(ClientError::Protocol(ProtocolError::new(
            ProtocolErrorCode::ResponseTooLarge,
            "sensor event response exceeds limit",
        )));
    }
    let response =
        decode_event_response(&response_buffer[..bytes]).map_err(ClientError::Protocol)?;
    if response.ok {
        response.batch.ok_or_else(|| {
            ClientError::Protocol(ProtocolError::new(
                ProtocolErrorCode::InvalidRequest,
                "sensor event success response has no batch",
            ))
        })
    } else {
        Err(ClientError::Remote(response.error.unwrap_or_else(|| {
            ProtocolError::new(
                ProtocolErrorCode::Internal,
                "sensor returned an empty event error",
            )
        })))
    }
}

fn query_once(
    pipe_name: &str,
    options: &ClientOptions,
    timeout: Duration,
) -> Result<SensorStatus, ClientError> {
    let handle = connect(pipe_name, timeout)?;
    let _handle = win::OwnedHandle(handle);
    let event = create_event()?;
    let _event = win::OwnedHandle(event);

    if options.verify_server {
        verify_server(handle, options)?;
    }

    let request = StatusRequest::status();
    let request_bytes = encode_request(&request).map_err(ClientError::Protocol)?;
    win::write(handle, event, &request_bytes, timeout).map_err(map_io)?;

    let mut response_buffer = vec![0u8; MAX_RESPONSE_BYTES + 1];
    let bytes = win::read(handle, event, &mut response_buffer, timeout).map_err(map_io)?;
    if bytes > MAX_RESPONSE_BYTES {
        return Err(ClientError::Protocol(ProtocolError::new(
            ProtocolErrorCode::ResponseTooLarge,
            "sensor response exceeds limit",
        )));
    }
    let response = decode_response(&response_buffer[..bytes]).map_err(ClientError::Protocol)?;
    if response.version != PROTOCOL_VERSION {
        return Err(ClientError::Protocol(ProtocolError::new(
            ProtocolErrorCode::UnsupportedVersion,
            "sensor response version mismatch",
        )));
    }
    if response.ok {
        response.status.ok_or_else(|| {
            ClientError::Protocol(ProtocolError::new(
                ProtocolErrorCode::InvalidRequest,
                "sensor success response has no status",
            ))
        })
    } else {
        Err(ClientError::Remote(response.error.unwrap_or_else(|| {
            ProtocolError::new(
                ProtocolErrorCode::Internal,
                "sensor returned an empty error",
            )
        })))
    }
}

fn connect(pipe_name: &str, timeout: Duration) -> Result<HANDLE, ClientError> {
    let wide_name = win::wide(pipe_name);
    let deadline = Instant::now() + timeout;
    loop {
        // SAFETY: wide_name is NUL-terminated and lives for the call.
        let handle = unsafe {
            CreateFileW(
                wide_name.as_ptr(),
                windows_sys::Win32::Foundation::GENERIC_READ
                    | windows_sys::Win32::Foundation::GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            match set_message_mode(handle) {
                Ok(()) => return Ok(handle),
                Err(error) if error.code == ERROR_PIPE_NOT_CONNECTED => {
                    // The server can publish the pipe just before it begins
                    // ConnectNamedPipe. Close this transient handle and retry
                    // within the caller's deadline.
                    // SAFETY: handle is valid and owned here.
                    unsafe {
                        CloseHandle(handle);
                    }
                    if Instant::now() >= deadline {
                        return Err(ClientError::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => {
                    // SAFETY: handle is valid and owned here.
                    unsafe {
                        CloseHandle(handle);
                    }
                    return Err(map_io(error));
                }
            }
        }

        let code = win::last_error_code();
        match code {
            ERROR_FILE_NOT_FOUND => return Err(ClientError::NotRunning),
            ERROR_PIPE_BUSY | ERROR_PIPE_NOT_CONNECTED => {
                let now = Instant::now();
                if now >= deadline {
                    return Err(ClientError::Timeout);
                }
                let remaining = deadline.saturating_duration_since(now);
                if code == ERROR_PIPE_BUSY {
                    let wait_ms = remaining.as_millis().min(u128::from(u32::MAX)) as u32;
                    // SAFETY: wide_name is a valid NUL-terminated pipe name.
                    let waited = unsafe { WaitNamedPipeW(wide_name.as_ptr(), wait_ms.max(1)) };
                    if waited == 0 {
                        let wait_error = win::last_error_code();
                        if wait_error == ERROR_SEM_TIMEOUT {
                            return Err(ClientError::Timeout);
                        }
                        return Err(map_io(IoError {
                            code: wait_error,
                            message: std::io::Error::from_raw_os_error(wait_error as i32)
                                .to_string(),
                            timeout: false,
                            more_data: false,
                        }));
                    }
                } else {
                    std::thread::sleep(remaining.min(Duration::from_millis(10)));
                }
                if Instant::now() >= deadline {
                    return Err(ClientError::Timeout);
                }
            }
            _ => {
                return Err(map_io(IoError {
                    code,
                    message: std::io::Error::from_raw_os_error(code as i32).to_string(),
                    timeout: false,
                    more_data: false,
                }));
            }
        }
    }
}

fn set_message_mode(handle: HANDLE) -> Result<(), IoError> {
    let mode = PIPE_READMODE_MESSAGE;
    // SAFETY: handle is a connected named-pipe handle.
    if unsafe { SetNamedPipeHandleState(handle, &mode, std::ptr::null(), std::ptr::null()) } == 0 {
        // SAFETY: the failed call set the thread-local last error.
        let code = unsafe { GetLastError() };
        return Err(IoError {
            code,
            message: std::io::Error::from_raw_os_error(code as i32).to_string(),
            timeout: false,
            more_data: false,
        });
    }
    Ok(())
}

fn create_event() -> Result<HANDLE, ClientError> {
    // SAFETY: no security attributes, manual-reset event, unnamed.
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event.is_null() {
        return Err(map_io(IoError::last_error()));
    }
    Ok(event)
}

fn verify_server(handle: HANDLE, options: &ClientOptions) -> Result<(), ClientError> {
    let (path, elevated) = win::server_process_identity(handle).map_err(map_io)?;
    let expected = options.expected_server_exe.as_deref().ok_or_else(|| {
        ClientError::Unavailable("expected sensor executable is not configured".to_owned())
    })?;
    let basename = Path::new(&path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if !basename.eq_ignore_ascii_case(expected) {
        return Err(ClientError::Unavailable(format!(
            "pipe server is not {expected} (got {basename})"
        )));
    }
    if options.require_elevation && !elevated {
        return Err(ClientError::Unavailable(
            "pipe server is not elevated".to_owned(),
        ));
    }
    Ok(())
}

fn map_io(error: IoError) -> ClientError {
    if error.timeout {
        return ClientError::Timeout;
    }
    if error.more_data {
        return ClientError::Protocol(ProtocolError::new(
            ProtocolErrorCode::ResponseTooLarge,
            "sensor response exceeds receive buffer",
        ));
    }
    match error.code {
        ERROR_FILE_NOT_FOUND => ClientError::NotRunning,
        ERROR_PIPE_NOT_CONNECTED | ERROR_BROKEN_PIPE => ClientError::ConnectionLost(error.message),
        ERROR_ACCESS_DENIED | ERROR_PIPE_BUSY => ClientError::Unavailable(error.message),
        _ => ClientError::Io {
            code: error.code,
            message: error.message,
        },
    }
}
