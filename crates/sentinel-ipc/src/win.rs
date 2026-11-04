//! Small, fail-closed wrappers around the Windows APIs used by the IPC layer.

use std::ffi::c_void;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_IO_PENDING, ERROR_MORE_DATA,
    ERROR_NOT_ALL_ASSIGNED, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LUID,
    LocalFree, SetLastError, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, GetTokenInformation, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW,
    SE_PRIVILEGE_ENABLED, SE_SECURITY_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_ELEVATION,
    TOKEN_PRIVILEGES, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{ConnectNamedPipe, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcess, OpenProcessToken, OpenThreadToken,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, ResetEvent, WaitForSingleObject,
};
use windows_sys::core::PWSTR;

const INFINITE: u32 = u32::MAX;
const MAX_SID_CHARS: usize = 256;

pub(crate) struct OwnedHandle(pub(crate) HANDLE);

impl OwnedHandle {
    pub(crate) fn is_valid(&self) -> bool {
        !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.is_valid() {
            // SAFETY: the handle is owned by this wrapper.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub(crate) const PRIVILEGE_RESTORE_ATTEMPTS: usize = 3;

#[derive(Debug)]
pub(crate) enum PrivilegeRestoreOutcome {
    Restored,
    RestoredAfterRetry,
    Failed(IoError),
}

pub(crate) struct SecurityPrivilegeGuard {
    token: OwnedHandle,
    luid: LUID,
    restore: bool,
}

impl SecurityPrivilegeGuard {
    fn restore_once(&mut self) -> Result<(), IoError> {
        if !self.restore {
            return Ok(());
        }
        let state = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: self.luid,
                Attributes: 0,
            }],
        };
        // SAFETY: token is a live process token handle and state is valid.
        unsafe {
            SetLastError(0);
        }
        if unsafe {
            AdjustTokenPrivileges(
                self.token.0,
                0,
                &state,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(IoError::last_error());
        }
        if last_error_code() == ERROR_NOT_ALL_ASSIGNED {
            return Err(IoError::last_error());
        }
        self.restore = false;
        Ok(())
    }

    pub(crate) fn restore(&mut self) -> PrivilegeRestoreOutcome {
        if !self.restore {
            return PrivilegeRestoreOutcome::Restored;
        }
        restore_with_retries(|| self.restore_once())
    }
}

impl Drop for SecurityPrivilegeGuard {
    fn drop(&mut self) {
        // The explicit lifecycle path uses restore(); Drop performs one final
        // best-effort attempt and never decides whether the sensor continues.
        let _ = self.restore_once();
    }
}

fn restore_with_retries<F>(mut attempt: F) -> PrivilegeRestoreOutcome
where
    F: FnMut() -> Result<(), IoError>,
{
    let mut last_error = None;
    for attempt_index in 0..PRIVILEGE_RESTORE_ATTEMPTS {
        match attempt() {
            Ok(()) => {
                return if attempt_index == 0 {
                    PrivilegeRestoreOutcome::Restored
                } else {
                    PrivilegeRestoreOutcome::RestoredAfterRetry
                };
            }
            Err(error) => {
                last_error = Some(error);
                if attempt_index + 1 < PRIVILEGE_RESTORE_ATTEMPTS {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
    PrivilegeRestoreOutcome::Failed(last_error.expect("at least one restore attempt"))
}

pub(crate) fn enable_security_privilege() -> Result<SecurityPrivilegeGuard, IoError> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: current process pseudo-handle and token out pointer are valid.
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    } == 0
    {
        return Err(IoError::last_error());
    }
    let token = OwnedHandle(token);
    let mut luid = LUID::default();
    // SAFETY: SE_SECURITY_NAME is a static NUL-terminated string.
    if unsafe { LookupPrivilegeValueW(std::ptr::null(), SE_SECURITY_NAME, &mut luid) } == 0 {
        return Err(IoError::last_error());
    }

    let state = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };
    let mut previous = TOKEN_PRIVILEGES::default();
    let mut returned = 0u32;
    // SAFETY: token is valid, state is valid, and previous is writable.
    unsafe {
        SetLastError(0);
    }
    if unsafe {
        AdjustTokenPrivileges(
            token.0,
            0,
            &state,
            std::mem::size_of::<TOKEN_PRIVILEGES>() as u32,
            &mut previous,
            &mut returned,
        )
    } == 0
    {
        return Err(IoError::last_error());
    }
    if last_error_code() == ERROR_NOT_ALL_ASSIGNED {
        return Err(IoError {
            code: ERROR_NOT_ALL_ASSIGNED,
            message: "SeSecurityPrivilege is not held".to_owned(),
            timeout: false,
            more_data: false,
        });
    }
    let restore = previous.PrivilegeCount == 0
        || (previous.Privileges[0].Attributes & SE_PRIVILEGE_ENABLED) == 0;
    Ok(SecurityPrivilegeGuard {
        token,
        luid,
        restore,
    })
}

#[derive(Debug, Clone)]
pub(crate) struct IoError {
    pub(crate) code: u32,
    pub(crate) message: String,
    pub(crate) timeout: bool,
    pub(crate) more_data: bool,
}

impl std::fmt::Display for IoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.code)
    }
}

impl IoError {
    pub(crate) fn last_error() -> Self {
        let code = last_error_code();
        Self {
            code,
            message: std::io::Error::from_raw_os_error(code as i32).to_string(),
            timeout: false,
            more_data: false,
        }
    }

    pub(crate) fn timeout() -> Self {
        Self {
            code: 0,
            message: "IPC operation timed out".to_owned(),
            timeout: true,
            more_data: false,
        }
    }

    pub(crate) fn more_data() -> Self {
        Self {
            code: ERROR_MORE_DATA,
            message: "IPC message exceeds the receive buffer".to_owned(),
            timeout: false,
            more_data: true,
        }
    }
}

pub(crate) fn last_error_code() -> u32 {
    // SAFETY: reads the thread-local last error.
    unsafe { GetLastError() }
}

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn timeout_millis(timeout: Duration) -> u32 {
    let millis = timeout.as_millis().min(u128::from(u32::MAX)) as u32;
    millis.max(1)
}

fn reset_event(event: HANDLE) {
    // SAFETY: event is a live event handle.
    unsafe {
        ResetEvent(event);
    }
}

fn wait_event(event: HANDLE, timeout: Duration) -> Result<(), IoError> {
    // SAFETY: event is a live event handle.
    match unsafe { WaitForSingleObject(event, timeout_millis(timeout)) } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(IoError::timeout()),
        _ => Err(IoError::last_error()),
    }
}

fn cancel_and_wait(handle: HANDLE, event: HANDLE, overlapped: &OVERLAPPED) {
    // SAFETY: the operation belongs to this handle and overlapped remains live.
    unsafe {
        CancelIoEx(handle, overlapped);
        WaitForSingleObject(event, INFINITE);
    }
}

fn finish_overlapped(
    handle: HANDLE,
    overlapped: &OVERLAPPED,
    bytes: &mut u32,
) -> Result<(), IoError> {
    // SAFETY: overlapped is live and the event has been signaled.
    if unsafe { GetOverlappedResult(handle, overlapped, bytes, 0) } == 0 {
        Err(IoError::last_error())
    } else {
        Ok(())
    }
}

pub(crate) fn connect(pipe: HANDLE, event: HANDLE, timeout: Duration) -> Result<(), IoError> {
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    reset_event(event);
    // SAFETY: pipe and overlapped are live for the duration of the call.
    let ok = unsafe { ConnectNamedPipe(pipe, &mut overlapped) };
    if ok != 0 {
        return Ok(());
    }
    let code = last_error_code();
    if code == ERROR_PIPE_CONNECTED {
        return Ok(());
    }
    if code != ERROR_IO_PENDING {
        return Err(IoError::last_error());
    }
    match wait_event(event, timeout) {
        Ok(()) => {
            let mut bytes = 0;
            finish_overlapped(pipe, &overlapped, &mut bytes)
        }
        Err(error) if error.timeout => {
            cancel_and_wait(pipe, event, &overlapped);
            Err(error)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn read(
    pipe: HANDLE,
    event: HANDLE,
    buffer: &mut [u8],
    timeout: Duration,
) -> Result<usize, IoError> {
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    reset_event(event);
    let mut bytes = 0;
    // SAFETY: pipe, buffer and overlapped are live for the call.
    let ok = unsafe {
        ReadFile(
            pipe,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            &mut bytes,
            &mut overlapped,
        )
    };
    if ok != 0 {
        return Ok(bytes as usize);
    }
    let code = last_error_code();
    if code == ERROR_MORE_DATA {
        return Err(IoError::more_data());
    }
    if code != ERROR_IO_PENDING {
        return Err(IoError::last_error());
    }
    match wait_event(event, timeout) {
        Ok(()) => {
            let mut transferred = 0;
            match finish_overlapped(pipe, &overlapped, &mut transferred) {
                Ok(()) => Ok(transferred as usize),
                Err(error) if error.code == ERROR_MORE_DATA => Err(IoError::more_data()),
                Err(error) => Err(error),
            }
        }
        Err(error) if error.timeout => {
            cancel_and_wait(pipe, event, &overlapped);
            Err(error)
        }
        Err(error) => Err(error),
    }
}

pub(crate) fn write(
    pipe: HANDLE,
    event: HANDLE,
    buffer: &[u8],
    timeout: Duration,
) -> Result<(), IoError> {
    let mut overlapped = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    reset_event(event);
    let mut bytes = 0;
    // SAFETY: pipe, buffer and overlapped are live for the call.
    let ok = unsafe {
        WriteFile(
            pipe,
            buffer.as_ptr(),
            buffer.len() as u32,
            &mut bytes,
            &mut overlapped,
        )
    };
    if ok != 0 {
        return Ok(());
    }
    if last_error_code() != ERROR_IO_PENDING {
        return Err(IoError::last_error());
    }
    match wait_event(event, timeout) {
        Ok(()) => {
            let mut transferred = 0;
            finish_overlapped(pipe, &overlapped, &mut transferred)
        }
        Err(error) if error.timeout => {
            cancel_and_wait(pipe, event, &overlapped);
            Err(error)
        }
        Err(error) => Err(error),
    }
}

fn current_user_sid_string() -> Result<String, IoError> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: current process pseudo-handle and token out pointer are valid.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(IoError::last_error());
    }
    let token = OwnedHandle(token);
    sid_string_from_token(token.0)
}

pub(crate) fn current_thread_user_sid_string() -> Result<String, IoError> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: current thread pseudo-handle and token out pointer are valid.
    if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) } == 0 {
        return Err(IoError::last_error());
    }
    let token = OwnedHandle(token);
    sid_string_from_token(token.0)
}

pub(crate) fn current_process_user_sid_string() -> Result<String, IoError> {
    current_user_sid_string()
}

fn sid_string_from_token(token: HANDLE) -> Result<String, IoError> {
    let mut needed = 0u32;
    // SAFETY: null buffer requests the required length.
    let ok = unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
    if ok != 0 || last_error_code() != ERROR_INSUFFICIENT_BUFFER {
        return Err(IoError::last_error());
    }
    let words = usize::try_from(needed).unwrap_or(0).div_ceil(8);
    if words == 0 {
        return Err(IoError {
            code: ERROR_INSUFFICIENT_BUFFER,
            message: "token user buffer is empty".to_owned(),
            timeout: false,
            more_data: false,
        });
    }
    let mut buffer = vec![0u64; words];
    // SAFETY: buffer is large enough for the requested TOKEN_USER.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(IoError::last_error());
    }
    // SAFETY: GetTokenInformation filled the buffer with a TOKEN_USER.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_to_string(user.User.Sid)
}

fn sid_to_string(sid: *mut c_void) -> Result<String, IoError> {
    let mut pointer: PWSTR = std::ptr::null_mut();
    // SAFETY: sid is valid for the lifetime of the token buffer.
    if unsafe { ConvertSidToStringSidW(sid, &mut pointer) } == 0 {
        return Err(IoError::last_error());
    }
    let mut length = 0usize;
    // SAFETY: ConvertSidToStringSidW returns a NUL-terminated allocation.
    while length < MAX_SID_CHARS && unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the allocation is NUL-terminated and LocalFree owns it.
    let value = unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(pointer, length)) };
    unsafe {
        LocalFree(pointer.cast());
    }
    Ok(value)
}

pub(crate) fn server_process_identity(pipe: HANDLE) -> Result<(String, bool), IoError> {
    let mut pid = 0u32;
    // SAFETY: pipe is a connected named-pipe handle.
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 {
        return Err(IoError::last_error());
    }
    // SAFETY: pid came from the connected pipe.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    let process = OwnedHandle(process);
    if !process.is_valid() {
        return Err(IoError::last_error());
    }
    let mut path_buffer = vec![0u16; 32_768];
    let mut path_len = path_buffer.len() as u32;
    // SAFETY: process is valid and path_buffer is writable.
    if unsafe { QueryFullProcessImageNameW(process.0, 0, path_buffer.as_mut_ptr(), &mut path_len) }
        == 0
    {
        return Err(IoError::last_error());
    }
    let path_len = usize::try_from(path_len)
        .unwrap_or(0)
        .min(path_buffer.len());
    let path = String::from_utf16_lossy(&path_buffer[..path_len])
        .trim_end_matches('\0')
        .to_owned();

    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: process is valid and token out pointer is writable.
    if unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token) } == 0 {
        return Err(IoError::last_error());
    }
    let token = OwnedHandle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    let size = std::mem::size_of::<TOKEN_ELEVATION>() as u32;
    // SAFETY: token is valid and elevation is correctly sized.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            std::ptr::addr_of_mut!(elevation).cast(),
            size,
            &mut returned,
        )
    } == 0
    {
        return Err(IoError::last_error());
    }
    Ok((path, elevation.TokenIsElevated != 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_error(code: u32) -> IoError {
        IoError {
            code,
            message: format!("test error {code}"),
            timeout: false,
            more_data: false,
        }
    }

    #[test]
    fn restore_retries_once_then_succeeds() {
        let mut attempts = 0;
        let outcome = restore_with_retries(|| {
            attempts += 1;
            if attempts == 1 {
                Err(test_error(5))
            } else {
                Ok(())
            }
        });
        assert!(matches!(
            outcome,
            PrivilegeRestoreOutcome::RestoredAfterRetry
        ));
        assert_eq!(attempts, 2);
    }

    #[test]
    fn restore_succeeds_on_first_attempt() {
        let mut attempts = 0;
        let outcome = restore_with_retries(|| {
            attempts += 1;
            Ok(())
        });
        assert!(matches!(outcome, PrivilegeRestoreOutcome::Restored));
        assert_eq!(attempts, 1);
    }

    #[test]
    fn restore_reports_persistent_failure_after_bounded_attempts() {
        let mut attempts = 0;
        let outcome = restore_with_retries(|| {
            attempts += 1;
            Err(test_error(1300))
        });
        match outcome {
            PrivilegeRestoreOutcome::Failed(error) => assert_eq!(error.code, 1300),
            other => panic!("unexpected outcome: {other:?}"),
        }
        assert_eq!(attempts, PRIVILEGE_RESTORE_ATTEMPTS);
    }
}
