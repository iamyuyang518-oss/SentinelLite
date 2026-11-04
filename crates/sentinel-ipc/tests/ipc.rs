use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sentinel_ipc::client::{ClientError, ClientOptions, query_status};
use sentinel_ipc::protocol::{
    MAX_REQUEST_BYTES, PermissionState, ProtocolErrorCode, SensorState, SensorStatus,
    decode_response,
};
use sentinel_ipc::server::{Server, ServerConfig, ServerError, StatusProvider};
use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, OPEN_EXISTING, SECURITY_ANONYMOUS, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
};

struct TestProvider {
    status: SensorStatus,
}

impl StatusProvider for TestProvider {
    fn status(&self) -> Result<SensorStatus, String> {
        Ok(self.status.clone())
    }
}

struct SlowProvider;

impl StatusProvider for SlowProvider {
    fn status(&self) -> Result<SensorStatus, String> {
        std::thread::sleep(Duration::from_millis(500));
        Ok(sample_status())
    }
}

fn sample_status() -> SensorStatus {
    SensorStatus {
        sensor_state: SensorState::Active,
        permission_state: PermissionState::Granted,
        events_received: 21,
        events_processed: 20,
        events_dropped: 0,
        events_lost: Some(0),
        realtime_buffers_lost: Some(0),
        decode_errors: 0,
        last_event_at: Some("2026-10-31T00:00:00.000Z".to_owned()),
        uptime_ms: 1234,
        health_ok: true,
        last_error: None,
    }
}

fn unique_pipe_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(
        r"\\.\pipe\SentinelLite-IPC-Test-{}-{nanos}",
        std::process::id()
    )
}

fn start_server(pipe_name: &str, io_timeout: Duration) -> Server {
    Server::start(
        ServerConfig::for_testing(pipe_name).with_io_timeout(io_timeout),
        Arc::new(TestProvider {
            status: sample_status(),
        }),
    )
    .expect("test server should start")
}

fn raw_client_with_flags(pipe_name: &str, security_flags: u32) -> File {
    let wide: Vec<u16> = pipe_name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: the name is NUL-terminated and the returned handle is owned by
    // the File below.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            security_flags,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(
        handle, INVALID_HANDLE_VALUE,
        "raw test client should connect"
    );
    // SAFETY: the handle is valid and ownership is transferred to File.
    unsafe { File::from_raw_handle(handle.cast()) }
}

fn raw_client(pipe_name: &str) -> File {
    raw_client_with_flags(pipe_name, SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION)
}

#[test]
fn status_query_round_trips_through_the_pipe() {
    let pipe_name = unique_pipe_name();
    let mut server = start_server(&pipe_name, Duration::from_millis(500));

    let status = query_status(&pipe_name, &ClientOptions::for_testing())
        .expect("status query should succeed");
    assert_eq!(status, sample_status());

    server.stop().expect("server should stop cleanly");
}

#[test]
fn duplicate_pipe_server_is_rejected_without_touching_the_first() {
    let pipe_name = unique_pipe_name();
    let mut server = start_server(&pipe_name, Duration::from_millis(500));

    let second = Server::start(
        ServerConfig::for_testing(&pipe_name),
        Arc::new(TestProvider {
            status: sample_status(),
        }),
    );
    match second {
        Err(ServerError::CreatePipe { code, .. }) => {
            assert_ne!(code, 0, "the original CreateNamedPipeW code must survive");
        }
        Err(other) => panic!("expected CreatePipe, got {other:?}"),
        Ok(_) => panic!("a second server must not share the pipe"),
    }

    let status = query_status(&pipe_name, &ClientOptions::for_testing())
        .expect("the original server should remain usable");
    assert_eq!(status, sample_status());

    server.stop().expect("server should stop cleanly");
}

#[test]
fn server_start_propagates_a_fatal_startup_error() {
    let config = ServerConfig::for_testing(unique_pipe_name()).with_startup_error_for_testing(
        ServerError::PrivilegeRestore("injected persistent restore failure".to_owned()),
    );
    let result = Server::start(
        config,
        Arc::new(TestProvider {
            status: sample_status(),
        }),
    );
    match result {
        Err(ServerError::PrivilegeRestore(message)) => {
            assert!(message.contains("persistent restore failure"));
        }
        Err(other) => panic!("expected PrivilegeRestore, got {other:?}"),
        Ok(_) => panic!("startup error must not produce a running server"),
    }
}

#[test]
fn server_start_propagates_a_fatal_create_pipe_error() {
    let config = ServerConfig::for_testing(unique_pipe_name()).with_startup_error_for_testing(
        ServerError::CreatePipe {
            code: 5,
            message: "injected create failure".to_owned(),
            privilege_restore_failed: true,
        },
    );
    let result = Server::start(
        config,
        Arc::new(TestProvider {
            status: sample_status(),
        }),
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
        Err(other) => panic!("expected CreatePipe, got {other:?}"),
        Ok(_) => panic!("startup error must not produce a running server"),
    }
}

#[test]
fn missing_pipe_is_reported_as_not_running() {
    let error = query_status(&unique_pipe_name(), &ClientOptions::for_testing())
        .expect_err("missing pipe must not look healthy");
    assert!(matches!(error, ClientError::NotRunning), "{error:?}");
}

#[test]
fn secure_client_rejects_a_non_sensor_server() {
    let pipe_name = unique_pipe_name();
    let mut server = start_server(&pipe_name, Duration::from_millis(500));

    let error = query_status(&pipe_name, &ClientOptions::secure())
        .expect_err("the test process is not the production sensor");
    match error {
        ClientError::Unavailable(message) => {
            assert!(
                message.contains("pipe server is not") || message.contains("not elevated"),
                "identity failure should be explicit: {message}"
            );
        }
        other => panic!("unexpected secure-client error: {other:?}"),
    }

    server.stop().expect("server should stop cleanly");
}

#[test]
fn malformed_request_returns_an_error_and_server_recovers() {
    let pipe_name = unique_pipe_name();
    let mut server = start_server(&pipe_name, Duration::from_millis(500));

    {
        let mut raw = raw_client(&pipe_name);
        raw.write_all(b"not-json").expect("raw write should work");
        let mut buffer = [0u8; 2048];
        let read = raw
            .read(&mut buffer)
            .expect("error response should be readable");
        let response = decode_response(&buffer[..read]).expect("error response should decode");
        assert!(!response.ok);
        assert_eq!(
            response.error.expect("error body").code,
            ProtocolErrorCode::InvalidRequest
        );
    }

    let status = query_status(&pipe_name, &ClientOptions::for_testing())
        .expect("server should remain usable");
    assert_eq!(status, sample_status());

    server.stop().expect("server should stop cleanly");
}

#[test]
fn oversized_request_returns_an_error_and_server_recovers() {
    let pipe_name = unique_pipe_name();
    let mut server = start_server(&pipe_name, Duration::from_millis(500));

    {
        let mut raw = raw_client(&pipe_name);
        raw.write_all(&vec![b'x'; MAX_REQUEST_BYTES + 1])
            .expect("raw write should work");
        let mut buffer = [0u8; 2048];
        let read = raw
            .read(&mut buffer)
            .expect("error response should be readable");
        let response = decode_response(&buffer[..read]).expect("error response should decode");
        assert!(!response.ok);
        assert_eq!(
            response.error.expect("error body").code,
            ProtocolErrorCode::RequestTooLarge
        );
    }

    let status = query_status(&pipe_name, &ClientOptions::for_testing())
        .expect("server should remain usable");
    assert_eq!(status, sample_status());

    server.stop().expect("server should stop cleanly");
}

#[test]
fn rapid_connect_close_does_not_kill_server() {
    let pipe_name = unique_pipe_name();
    let mut server = start_server(&pipe_name, Duration::from_millis(500));

    for _ in 0..8 {
        drop(raw_client(&pipe_name));
        std::thread::sleep(Duration::from_millis(25));
    }

    let status = query_status(&pipe_name, &ClientOptions::for_testing())
        .expect("server should survive rapid connect/close");
    assert_eq!(status, sample_status());

    server.stop().expect("server should stop cleanly");
}

#[test]
fn idle_client_times_out_and_server_recovers() {
    let pipe_name = unique_pipe_name();
    let mut server = start_server(&pipe_name, Duration::from_millis(200));

    let _idle = raw_client(&pipe_name);
    std::thread::sleep(Duration::from_millis(350));

    let status = query_status(&pipe_name, &ClientOptions::for_testing())
        .expect("server should accept a new client after an idle timeout");
    assert_eq!(status, sample_status());

    server.stop().expect("server should stop cleanly");
}

#[test]
fn client_timeout_is_not_treated_as_a_status() {
    let pipe_name = unique_pipe_name();
    let mut server = Server::start(
        ServerConfig::for_testing(&pipe_name).with_io_timeout(Duration::from_secs(2)),
        Arc::new(SlowProvider),
    )
    .expect("slow test server should start");

    let mut options = ClientOptions::for_testing();
    options.timeout = Duration::from_millis(100);
    let error = query_status(&pipe_name, &options).expect_err("slow provider must time out");
    assert!(matches!(error, ClientError::Timeout), "{error:?}");

    server.stop().expect("server should stop cleanly");
}

#[test]
fn anonymous_client_is_rejected_and_server_recovers() {
    let pipe_name = unique_pipe_name();
    let mut server = start_server(&pipe_name, Duration::from_millis(500));

    {
        let mut raw = raw_client_with_flags(&pipe_name, SECURITY_SQOS_PRESENT | SECURITY_ANONYMOUS);
        raw.write_all(b"{\"version\":1,\"request\":\"status\"}")
            .expect("raw write should work");
        let mut buffer = [0u8; 2048];
        let read = raw
            .read(&mut buffer)
            .expect("auth error should be readable");
        let response = decode_response(&buffer[..read]).expect("auth error should decode");
        assert_eq!(
            response.error.expect("auth error body").code,
            ProtocolErrorCode::Unauthorized
        );
    }

    let status = query_status(&pipe_name, &ClientOptions::for_testing())
        .expect("server should remain usable");
    assert_eq!(status, sample_status());

    server.stop().expect("server should stop cleanly");
}
