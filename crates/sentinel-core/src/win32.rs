use std::io;
use std::path::Path;

use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;

use crate::model::PathState;
use crate::util::{is_local_absolute_path, path_state_from_io_error};

pub(crate) fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn string_from_wide_until_nul(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

pub(crate) fn is_access_denied(error: &io::Error) -> bool {
    error.raw_os_error() == Some(5)
}

/// Expands Windows environment variables. Returns None when the value cannot be resolved.
pub(crate) fn expand_environment(value: &str) -> Option<String> {
    let source = wide_null(value);
    let required = unsafe { ExpandEnvironmentStringsW(source.as_ptr(), std::ptr::null_mut(), 0) };
    if required == 0 {
        return None;
    }

    let mut buffer = vec![0u16; required as usize];
    let written = unsafe {
        ExpandEnvironmentStringsW(source.as_ptr(), buffer.as_mut_ptr(), buffer.len() as u32)
    };
    if written == 0 || written > buffer.len() as u32 {
        return None;
    }

    let expanded = string_from_wide_until_nul(&buffer);
    if expanded.contains('%') {
        // An unresolved variable would make existence checks misleading.
        return None;
    }
    Some(expanded)
}

/// Resolves and probes a local path without collapsing access errors into "missing".
///
/// Network shares, device paths, and relative paths are deliberately not touched.
pub(crate) fn probe_local_path(raw_path: &str) -> (Option<String>, PathState) {
    let Some(expanded) = expand_environment(raw_path) else {
        return (None, PathState::Unknown);
    };
    if !is_local_absolute_path(&expanded) {
        return (Some(expanded), PathState::Unknown);
    }

    let state = match std::fs::metadata(Path::new(&expanded)) {
        Ok(_) => PathState::Present,
        Err(error) => path_state_from_io_error(&error),
    };
    (Some(expanded), state)
}

pub(crate) fn filetime_ticks(dw_low_date_time: u32, dw_high_date_time: u32) -> u64 {
    (u64::from(dw_high_date_time) << 32) | u64::from(dw_low_date_time)
}
