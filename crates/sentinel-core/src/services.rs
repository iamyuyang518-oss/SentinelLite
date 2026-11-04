//! Read-only Windows Service Control Manager inventory.
//!
//! The scanner never starts, stops, creates, deletes, or reconfigures a service. It reads the
//! service name/display name/state from the SCM enumeration and, when permitted, the start type,
//! account and executable path from `QueryServiceConfigW`. Arguments are deliberately discarded.

use std::time::{Duration, Instant};

use windows::Win32::System::Services::{
    CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW, OpenSCManagerW,
    OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, SC_ENUM_PROCESS_INFO,
    SC_MANAGER_ENUMERATE_SERVICE, SERVICE_ADAPTER, SERVICE_AUTO_START, SERVICE_BOOT_START,
    SERVICE_CONTINUE_PENDING, SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_DRIVER,
    SERVICE_FILE_SYSTEM_DRIVER, SERVICE_KERNEL_DRIVER, SERVICE_PAUSE_PENDING, SERVICE_PAUSED,
    SERVICE_QUERY_CONFIG, SERVICE_RECOGNIZER_DRIVER, SERVICE_RUNNING, SERVICE_START_PENDING,
    SERVICE_STATE_ALL, SERVICE_STOP_PENDING, SERVICE_STOPPED, SERVICE_SYSTEM_START, SERVICE_WIN32,
    SERVICE_WIN32_OWN_PROCESS, SERVICE_WIN32_SHARE_PROCESS,
};
use windows::Win32::System::Services::{
    SC_HANDLE, SERVICE_START_TYPE, SERVICE_STATUS_CURRENT_STATE,
};
use windows::core::PCWSTR;

use crate::limits::{
    MAX_SERVICE_ACCOUNT_CHARS, MAX_SERVICE_ITEMS, MAX_SERVICE_NAME_CHARS, MAX_SERVICE_PATH_CHARS,
    bound_text,
};
use crate::model::{
    DataIssue, InventoryObservation, ServiceInventoryResult, ServiceStartType, ServiceState,
    ServiceSummary, ServiceType,
};
use crate::util::now_rfc3339;

const MAX_ENUM_BUFFER_BYTES: usize = 4 * 1024 * 1024;
const INITIAL_ENUM_BUFFER_BYTES: usize = 256 * 1024;
// `QueryServiceConfigW` returns RPC_X_BAD_STUB_DATA (1783) for a 64 KiB buffer on the tested
// Windows 11 build; keep the buffer below that boundary and grow only to the reported need.
const MAX_CONFIG_BUFFER_BYTES: usize = 32 * 1024;
const INITIAL_CONFIG_BUFFER_BYTES: usize = 8 * 1024;
const TIME_LIMIT: Duration = Duration::from_secs(8);
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_SERVICE_DOES_NOT_EXIST: u32 = 1_060;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_MORE_DATA: u32 = 234;
const SERVICE_INTERACTIVE_PROCESS: u32 = 0x100;
const MAX_ENUM_PAGES: usize = 64;

struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

#[derive(Debug, Clone)]
struct RawServiceEntry {
    name: String,
    display_name: String,
    state: ServiceState,
    service_types: Vec<ServiceType>,
}

#[derive(Debug, Clone)]
struct ServiceConfigRead {
    start_type: ServiceStartType,
    service_types: Vec<ServiceType>,
    account: Option<String>,
    executable_path: Option<String>,
    path_unparseable: bool,
}

pub fn list_services() -> ServiceInventoryResult {
    let started = Instant::now();
    let checked_at = now_rfc3339();
    let manager = match unsafe {
        OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE)
    } {
        Ok(handle) => ScHandle(handle),
        Err(error) => {
            return failed_result(
                checked_at,
                observation_for_code(hresult_code(&error)),
                vec![issue(
                    "SERVICE-INVENTORY-UNAVAILABLE",
                    format!(
                        "无法打开服务控制管理器（HRESULT 0x{:08X}）；服务列表未知。",
                        hresult_code(&error)
                    ),
                    "SCM",
                )],
            );
        }
    };

    let mut entries = Vec::new();
    let mut truncated = false;
    let mut observation = InventoryObservation::Observed;
    let mut issues = Vec::new();
    let mut resume = 0u32;
    let mut buffer_bytes = INITIAL_ENUM_BUFFER_BYTES;
    let mut pages = 0usize;

    loop {
        pages += 1;
        if pages > MAX_ENUM_PAGES {
            truncated = true;
            observation = InventoryObservation::Partial;
            issues.push(issue(
                "SERVICE-INVENTORY-PAGE-LIMIT",
                "服务枚举超过 64 页安全上限，已停止读取；结果不完整。",
                "SCM",
            ));
            break;
        }
        if started.elapsed() >= TIME_LIMIT {
            truncated = true;
            observation = InventoryObservation::Partial;
            issues.push(issue(
                "SERVICE-INVENTORY-TIME-LIMIT",
                "服务枚举超过 8 秒安全上限，已停止继续读取；结果可能不完整。",
                "SCM",
            ));
            break;
        }

        let word_count = buffer_bytes.div_ceil(8);
        let mut words = vec![0u64; word_count];
        let mut needed = 0u32;
        let mut returned = 0u32;
        let slice = unsafe {
            std::slice::from_raw_parts_mut(
                words.as_mut_ptr().cast::<u8>(),
                words.len() * std::mem::size_of::<u64>(),
            )
        };

        let result = unsafe {
            EnumServicesStatusExW(
                manager.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32 | SERVICE_DRIVER,
                SERVICE_STATE_ALL,
                Some(slice),
                &mut needed,
                &mut returned,
                Some(&mut resume),
                PCWSTR::null(),
            )
        };

        match result {
            Ok(()) => {
                let base = words.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>();
                for index in 0..returned as usize {
                    if entries.len() >= MAX_SERVICE_ITEMS {
                        truncated = true;
                        break;
                    }
                    let raw = unsafe { std::ptr::read_unaligned(base.add(index)) };
                    let Some(name) =
                        (unsafe { pwstr_to_string(raw.lpServiceName, MAX_SERVICE_NAME_CHARS) })
                    else {
                        issues.push(issue(
                            "SERVICE-ENTRY-NAME-UNAVAILABLE",
                            "服务枚举返回了没有可读名称的条目，已跳过。",
                            "SCM",
                        ));
                        continue;
                    };
                    let display_name =
                        unsafe { pwstr_to_string(raw.lpDisplayName, MAX_SERVICE_NAME_CHARS) }
                            .unwrap_or_else(|| name.clone());
                    entries.push(RawServiceEntry {
                        name,
                        display_name,
                        state: map_service_state(raw.ServiceStatusProcess.dwCurrentState),
                        service_types: map_service_types(raw.ServiceStatusProcess.dwServiceType.0),
                    });
                }

                if entries.len() >= MAX_SERVICE_ITEMS {
                    truncated = true;
                    break;
                }
                if returned == 0 || resume == 0 {
                    break;
                }
            }
            Err(error) => {
                let code = hresult_code(&error);
                if matches!(code & 0xFFFF, ERROR_INSUFFICIENT_BUFFER | ERROR_MORE_DATA) {
                    let requested = usize::try_from(needed).unwrap_or(0);
                    if requested > MAX_ENUM_BUFFER_BYTES {
                        truncated = true;
                        observation = InventoryObservation::Partial;
                        issues.push(issue(
                            "SERVICE-INVENTORY-BUFFER-LIMIT",
                            "服务枚举数据超过 4 MiB 安全上限，已停止读取；结果不完整。",
                            "SCM",
                        ));
                        break;
                    }
                    buffer_bytes = requested.max(buffer_bytes.saturating_mul(2));
                    continue;
                }
                observation = observation_for_code(code);
                issues.push(issue(
                    "SERVICE-INVENTORY-ENUMERATION-FAILED",
                    format!("服务枚举失败（HRESULT 0x{code:08X}）；已返回的部分条目可能不完整。"),
                    "SCM",
                ));
                break;
            }
        }
    }

    let elapsed = started.elapsed();
    let mut items = Vec::with_capacity(entries.len());
    let mut partial_items = 0usize;
    let mut denied_items = 0usize;
    for entry in entries {
        if started.elapsed() >= TIME_LIMIT {
            if !truncated {
                truncated = true;
                if observation == InventoryObservation::Observed {
                    observation = InventoryObservation::Partial;
                }
                issues.push(issue(
                    "SERVICE-INVENTORY-TIME-LIMIT",
                    "服务配置读取超过 8 秒安全上限；结果可能不完整。",
                    "SCM",
                ));
            }
            break;
        }
        let config = query_service_config(manager.0, &entry.name);
        let (summary, item_issue, item_denied) = build_service_summary(entry, config);
        if item_issue {
            partial_items += 1;
        }
        if item_denied {
            denied_items += 1;
        }
        items.push(summary);
    }

    if partial_items > 0 {
        if observation == InventoryObservation::Observed {
            observation = InventoryObservation::Partial;
        }
        issues.push(issue(
            "SERVICE-INVENTORY-PARTIAL",
            format!("{partial_items} 个服务的配置字段未能完整读取；这些条目保持部分未知。"),
            "SCM",
        ));
    }
    if denied_items > 0 {
        if observation == InventoryObservation::Observed {
            observation = InventoryObservation::Partial;
        }
        issues.push(issue(
            "SERVICE-INVENTORY-ACCESS-DENIED",
            format!("{denied_items} 个服务因权限不足无法读取配置；未把它们当作正常。"),
            "SCM",
        ));
    }
    if truncated {
        issues.push(issue(
            "SERVICE-INVENTORY-TRUNCATED",
            format!("服务结果达到安全上限（{MAX_SERVICE_ITEMS} 项或时间上限），不是完整列表。"),
            "SCM",
        ));
    }
    if items.is_empty() && observation == InventoryObservation::Observed {
        issues.push(issue(
            "SERVICE-INVENTORY-EMPTY",
            "服务枚举没有返回可显示条目；这不代表系统中没有服务。",
            "SCM",
        ));
    }
    if elapsed >= TIME_LIMIT && !truncated {
        truncated = true;
        issues.push(issue(
            "SERVICE-INVENTORY-TIME-LIMIT",
            "服务读取超过 8 秒安全上限；结果可能不完整。",
            "SCM",
        ));
    }

    ServiceInventoryResult {
        checked_at,
        observation,
        items,
        issues,
        limitations: vec![
            "这是某一时刻的 SCM 只读快照，不是实时监控，也不会自动刷新。".to_string(),
            format!(
                "最多返回 {MAX_SERVICE_ITEMS} 个服务；达到上限、缓冲上限或 8 秒时间上限会标记结果不完整。"
            ),
            "只显示服务可执行文件路径，不保留或展示启动参数/完整命令行。".to_string(),
            "普通用户可能无法读取部分服务的配置、账户或路径；这些条目显示为部分/无权限，不会当作正常。".to_string(),
            "服务状态与启动类型来自 SCM；本页不启动、停止、创建、删除或修改任何服务。".to_string(),
            "不判断服务是否恶意；服务名称、发布者或路径本身不能作为恶意结论。".to_string(),
        ],
        truncated,
    }
}

fn build_service_summary(
    entry: RawServiceEntry,
    config: Result<ServiceConfigRead, u32>,
) -> (ServiceSummary, bool, bool) {
    let mut issues = Vec::new();
    let mut observation = InventoryObservation::Observed;
    let (start_type, service_types, account, executable_path, path_unparseable) = match config {
        Ok(config) => (
            config.start_type,
            config.service_types,
            config.account,
            config.executable_path,
            config.path_unparseable,
        ),
        Err(code) => {
            observation = observation_for_code(code);
            let message = if code & 0xFFFF == ERROR_ACCESS_DENIED {
                "没有权限读取服务配置；启动类型、账户和路径保持未知。".to_string()
            } else {
                format!("服务配置读取失败（HRESULT 0x{code:08X}）；启动类型、账户和路径保持未知。")
            };
            issues.push(issue("SERVICE-CONFIG-UNREADABLE", message, "SCM"));
            (
                ServiceStartType::Unknown,
                entry.service_types.clone(),
                None,
                None,
                false,
            )
        }
    };

    if path_unparseable && observation == InventoryObservation::Observed {
        observation = InventoryObservation::Partial;
        issues.push(issue(
            "SERVICE-BINARY-PATH-UNPARSEABLE",
            "服务配置提供了无法安全解析的可执行文件路径；路径保持未知。",
            "SCM",
        ));
    }
    // A successful config query with an empty account is common for kernel drivers and means
    // "not provided", not "read failure"; only a failed query makes the account unknown.

    let item_issue = !matches!(
        observation,
        InventoryObservation::Observed | InventoryObservation::AccessDenied
    );
    let denied = observation == InventoryObservation::AccessDenied;
    (
        ServiceSummary {
            id: format!("service:{}", entry.name),
            name: entry.name,
            display_name: entry.display_name,
            state: entry.state,
            start_type,
            service_types,
            account,
            executable_path,
            observation,
            issues,
        },
        item_issue,
        denied,
    )
}

fn query_service_config(manager: SC_HANDLE, name: &str) -> Result<ServiceConfigRead, u32> {
    let wide_name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let service = match unsafe {
        OpenServiceW(
            manager,
            PCWSTR::from_raw(wide_name.as_ptr()),
            SERVICE_QUERY_CONFIG,
        )
    } {
        Ok(handle) => ScHandle(handle),
        Err(error) => return Err(hresult_code(&error)),
    };

    let mut buffer_bytes = INITIAL_CONFIG_BUFFER_BYTES;
    loop {
        let mut words = vec![0u64; buffer_bytes.div_ceil(8)];
        let mut needed = 0u32;
        let result = unsafe {
            QueryServiceConfigW(
                service.0,
                Some(words.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>()),
                (words.len() * std::mem::size_of::<u64>()) as u32,
                &mut needed,
            )
        };
        match result {
            Ok(()) => {
                let config = unsafe { &*(words.as_ptr().cast::<QUERY_SERVICE_CONFIGW>()) };
                let account = unsafe {
                    pwstr_to_string(config.lpServiceStartName, MAX_SERVICE_ACCOUNT_CHARS)
                };
                let raw_path =
                    unsafe { pwstr_to_string(config.lpBinaryPathName, MAX_SERVICE_PATH_CHARS) };
                let (executable_path, path_unparseable) = match raw_path.as_deref() {
                    Some(raw) => match parse_service_binary_path(raw) {
                        Some(path) => (Some(path), false),
                        None => (None, true),
                    },
                    None => (None, false),
                };
                return Ok(ServiceConfigRead {
                    start_type: map_start_type(config.dwStartType),
                    service_types: map_service_types(config.dwServiceType.0),
                    account,
                    executable_path,
                    path_unparseable,
                });
            }
            Err(error) => {
                let code = hresult_code(&error);
                if matches!(code & 0xFFFF, ERROR_INSUFFICIENT_BUFFER | ERROR_MORE_DATA)
                    && needed > 0
                    && usize::try_from(needed).unwrap_or(usize::MAX) <= MAX_CONFIG_BUFFER_BYTES
                {
                    buffer_bytes = usize::try_from(needed).unwrap_or(buffer_bytes);
                    continue;
                }
                return Err(code);
            }
        }
    }
}

fn failed_result(
    checked_at: String,
    observation: InventoryObservation,
    issues: Vec<DataIssue>,
) -> ServiceInventoryResult {
    ServiceInventoryResult {
        checked_at,
        observation,
        items: Vec::new(),
        issues,
        limitations: vec![
            "服务列表不可读；空结果不代表没有服务。".to_string(),
            "不启动、停止、创建、删除或修改任何服务。".to_string(),
        ],
        truncated: false,
    }
}

fn issue(code: &str, message: impl Into<String>, source: &str) -> DataIssue {
    DataIssue {
        code: code.to_string(),
        message: message.into(),
        source: source.to_string(),
    }
}

fn map_service_state(state: SERVICE_STATUS_CURRENT_STATE) -> ServiceState {
    match state {
        SERVICE_STOPPED => ServiceState::Stopped,
        SERVICE_START_PENDING => ServiceState::StartPending,
        SERVICE_STOP_PENDING => ServiceState::StopPending,
        SERVICE_RUNNING => ServiceState::Running,
        SERVICE_CONTINUE_PENDING => ServiceState::ContinuePending,
        SERVICE_PAUSE_PENDING => ServiceState::PausePending,
        SERVICE_PAUSED => ServiceState::Paused,
        _ => ServiceState::Unknown,
    }
}

fn map_start_type(start_type: SERVICE_START_TYPE) -> ServiceStartType {
    match start_type {
        SERVICE_BOOT_START => ServiceStartType::Boot,
        SERVICE_SYSTEM_START => ServiceStartType::System,
        SERVICE_AUTO_START => ServiceStartType::Auto,
        SERVICE_DEMAND_START => ServiceStartType::Demand,
        SERVICE_DISABLED => ServiceStartType::Disabled,
        _ => ServiceStartType::Unknown,
    }
}

fn map_service_types(bits: u32) -> Vec<ServiceType> {
    let mut types = Vec::new();
    if bits & SERVICE_KERNEL_DRIVER.0 != 0 {
        types.push(ServiceType::KernelDriver);
    }
    if bits & SERVICE_FILE_SYSTEM_DRIVER.0 != 0 {
        types.push(ServiceType::FileSystemDriver);
    }
    if bits & SERVICE_ADAPTER.0 != 0 {
        types.push(ServiceType::Adapter);
    }
    if bits & SERVICE_RECOGNIZER_DRIVER.0 != 0 {
        types.push(ServiceType::RecognizerDriver);
    }
    if bits & SERVICE_WIN32_OWN_PROCESS.0 != 0 {
        types.push(ServiceType::OwnProcess);
    }
    if bits & SERVICE_WIN32_SHARE_PROCESS.0 != 0 {
        types.push(ServiceType::ShareProcess);
    }
    if bits & SERVICE_INTERACTIVE_PROCESS != 0 {
        types.push(ServiceType::InteractiveProcess);
    }
    let known = SERVICE_KERNEL_DRIVER.0
        | SERVICE_FILE_SYSTEM_DRIVER.0
        | SERVICE_ADAPTER.0
        | SERVICE_RECOGNIZER_DRIVER.0
        | SERVICE_WIN32_OWN_PROCESS.0
        | SERVICE_WIN32_SHARE_PROCESS.0
        | SERVICE_INTERACTIVE_PROCESS;
    if bits & !known != 0 || types.is_empty() {
        types.push(ServiceType::Other);
    }
    types
}

/// Extracts only the executable path from a service binary command line. Arguments are never
/// retained. Returns None for ambiguous unquoted paths with spaces and no executable extension.
fn parse_service_binary_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix('"') {
        let closing = rest.find('"')?;
        let path = rest.get(..closing)?.trim();
        return if path.is_empty() {
            None
        } else {
            Some(path.to_string())
        };
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let first = *tokens.first()?;
    if tokens.len() == 1 || has_service_path_extension(first) {
        return Some(first.to_string());
    }
    if looks_like_path_start(first)
        && tokens.get(1).is_some_and(|token| {
            (token.starts_with('-') || token.starts_with('/'))
                && !token.contains('\\')
                && !has_service_path_extension(token)
        })
    {
        // Extensionless service binaries such as `svchost -k ...` are valid.
        return Some(first.to_string());
    }
    if first.starts_with(r"\??\") && has_service_path_extension(trimmed) {
        // Device-namespace paths may legitimately contain spaces and no arguments.
        return Some(trimmed.to_string());
    }
    None
}

fn looks_like_path_start(value: &str) -> bool {
    value.contains('\\') || value.contains('/') || value.starts_with('%')
}

fn has_service_path_extension(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [".exe", ".sys", ".dll", ".com", ".bat", ".cmd"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn hresult_code(error: &windows::core::Error) -> u32 {
    error.code().0 as u32
}

fn observation_for_code(code: u32) -> InventoryObservation {
    match code & 0xFFFF {
        ERROR_ACCESS_DENIED => InventoryObservation::AccessDenied,
        ERROR_SERVICE_DOES_NOT_EXIST => InventoryObservation::Unavailable,
        _ => InventoryObservation::Error,
    }
}

unsafe fn pwstr_to_string(value: windows::core::PWSTR, max_chars: usize) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let mut length = 0usize;
    while length <= max_chars && unsafe { *value.0.add(length) } != 0 {
        length += 1;
    }
    if length == 0 {
        return None;
    }
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(value.0, length) });
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(bound_text(trimmed, max_chars).value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Services::{
        SERVICE_START_TYPE, SERVICE_STATUS_CURRENT_STATE, SERVICE_SYSTEM_START,
        SERVICE_WIN32_OWN_PROCESS,
    };

    #[test]
    fn service_state_mapping_covers_known_and_unknown_values() {
        assert_eq!(map_service_state(SERVICE_RUNNING), ServiceState::Running);
        assert_eq!(map_service_state(SERVICE_STOPPED), ServiceState::Stopped);
        assert_eq!(
            map_service_state(SERVICE_STATUS_CURRENT_STATE(99)),
            ServiceState::Unknown
        );
    }

    #[test]
    fn start_type_mapping_covers_known_and_unknown_values() {
        assert_eq!(map_start_type(SERVICE_AUTO_START), ServiceStartType::Auto);
        assert_eq!(
            map_start_type(SERVICE_SYSTEM_START),
            ServiceStartType::System
        );
        assert_eq!(
            map_start_type(SERVICE_START_TYPE(99)),
            ServiceStartType::Unknown
        );
    }

    #[test]
    fn service_type_mapping_preserves_bits_and_unknown_bits() {
        assert_eq!(
            map_service_types(SERVICE_WIN32_OWN_PROCESS.0),
            vec![ServiceType::OwnProcess]
        );
        assert_eq!(
            map_service_types(SERVICE_WIN32_OWN_PROCESS.0 | SERVICE_KERNEL_DRIVER.0),
            vec![ServiceType::KernelDriver, ServiceType::OwnProcess]
        );
        assert_eq!(map_service_types(0x4000), vec![ServiceType::Other]);
    }

    #[test]
    fn service_binary_path_keeps_only_the_executable() {
        assert_eq!(
            parse_service_binary_path(r#""C:\Program Files\Vendor\svc.exe" -k netsvcs"#),
            Some(r"C:\Program Files\Vendor\svc.exe".to_string())
        );
        assert_eq!(
            parse_service_binary_path(r"C:\Windows\System32\svchost.exe -k netsvcs"),
            Some(r"C:\Windows\System32\svchost.exe".to_string())
        );
        assert_eq!(
            parse_service_binary_path(r"\SystemRoot\System32\drivers\foo.sys"),
            Some(r"\SystemRoot\System32\drivers\foo.sys".to_string())
        );
        assert_eq!(
            parse_service_binary_path(r"C:\Program Files\Vendor\svc.exe -k netsvcs"),
            None
        );
        assert_eq!(
            parse_service_binary_path(r"C:\somewhere -argument\path.exe"),
            None
        );
        assert_eq!(
            parse_service_binary_path(r"C:\WINDOWS\system32\svchost -k douyin_performance_service"),
            Some(r"C:\WINDOWS\system32\svchost".to_string())
        );
        assert_eq!(
            parse_service_binary_path(
                r"\??\C:\Program Files\Denuvo Anti-Cheat\denuvo-anti-cheat.sys"
            ),
            Some(r"\??\C:\Program Files\Denuvo Anti-Cheat\denuvo-anti-cheat.sys".to_string())
        );
        assert_eq!(parse_service_binary_path("   "), None);
    }

    #[test]
    fn observation_for_code_distinguishes_access_denied_and_missing_service() {
        assert_eq!(
            observation_for_code(0x8007_0005),
            InventoryObservation::AccessDenied
        );
        assert_eq!(
            observation_for_code(0x8007_0424),
            InventoryObservation::Unavailable
        );
        assert_eq!(
            observation_for_code(0x8000_4000),
            InventoryObservation::Error
        );
    }

    #[test]
    fn failed_config_never_reports_a_known_start_type() {
        let (summary, partial, denied) = build_service_summary(
            RawServiceEntry {
                name: "Example".to_string(),
                display_name: "Example Service".to_string(),
                state: ServiceState::Running,
                service_types: vec![ServiceType::OwnProcess],
            },
            Err(ERROR_ACCESS_DENIED),
        );

        assert_eq!(summary.observation, InventoryObservation::AccessDenied);
        assert_eq!(summary.start_type, ServiceStartType::Unknown);
        assert_eq!(summary.account, None);
        assert_eq!(summary.executable_path, None);
        assert!(!partial);
        assert!(denied);
    }

    #[test]
    fn absent_binary_path_is_not_a_read_failure() {
        let (summary, partial, denied) = build_service_summary(
            RawServiceEntry {
                name: "Example".to_string(),
                display_name: "Example Service".to_string(),
                state: ServiceState::Running,
                service_types: vec![ServiceType::OwnProcess],
            },
            Ok(ServiceConfigRead {
                start_type: ServiceStartType::Auto,
                service_types: vec![ServiceType::OwnProcess],
                account: None,
                executable_path: None,
                path_unparseable: false,
            }),
        );

        assert_eq!(summary.observation, InventoryObservation::Observed);
        assert!(!partial);
        assert!(!denied);
        assert_eq!(summary.start_type, ServiceStartType::Auto);
        assert!(summary.issues.is_empty());
    }

    #[test]
    fn unparseable_binary_path_is_partial_and_unknown() {
        let (summary, partial, denied) = build_service_summary(
            RawServiceEntry {
                name: "Example".to_string(),
                display_name: "Example Service".to_string(),
                state: ServiceState::Running,
                service_types: vec![ServiceType::OwnProcess],
            },
            Ok(ServiceConfigRead {
                start_type: ServiceStartType::Auto,
                service_types: vec![ServiceType::OwnProcess],
                account: Some("LocalSystem".to_string()),
                executable_path: None,
                path_unparseable: true,
            }),
        );

        assert_eq!(summary.observation, InventoryObservation::Partial);
        assert!(partial);
        assert!(!denied);
        assert!(
            summary
                .issues
                .iter()
                .any(|issue| issue.code == "SERVICE-BINARY-PATH-UNPARSEABLE")
        );
    }
}
