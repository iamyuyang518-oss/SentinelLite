use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_NO_MORE_FILES, FILETIME, GetLastError, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows_sys::Win32::System::Threading::{
    GetProcessAffinityMask, GetProcessTimes, OpenProcess, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

use crate::error::CoreError;
use crate::limits::{
    MAX_PROCESS_ITEMS, MAX_PROCESS_NAME_CHARS, MAX_PROCESS_PATH_CHARS, bound_text,
};
use crate::model::{
    AffinityInspection, AffinityResult, AffinitySourceTrace, CpuMask, CpuUsageSample, DataIssue,
    InspectionState, ProcessListResult, ProcessPathAccess, ProcessSummary,
};
use crate::rules::{AffinityFacts, assess_affinity, assess_process_overall};
use crate::signature::verify_file;
use crate::util::{
    ProcessIdentity, calculate_cpu_percent, filetime_ticks_to_rfc3339, is_affinity_restricted,
    mask_to_cpu_indices, mask_to_hex, now_rfc3339,
};
use crate::win32::{filetime_ticks, string_from_wide_until_nul};

const CPU_SAMPLE_WINDOW: Duration = Duration::from_millis(500);

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn from_valid(handle: HANDLE) -> Option<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            None
        } else {
            Some(Self(handle))
        }
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[derive(Clone)]
struct ProcessEntry {
    pid: u32,
    parent_pid: u32,
    name: String,
}

struct CpuTimes {
    creation_ticks: u64,
    cpu_ticks: u64,
}

struct AffinityRead {
    result: AffinityResult,
    process_mask: u64,
    system_mask: u64,
}

struct ProcessMetadata {
    executable_path: Option<String>,
    path_access: ProcessPathAccess,
    identity: Option<String>,
    start_time: Option<String>,
}

pub fn list_processes() -> Result<ProcessListResult, CoreError> {
    let (raw_processes, truncated) = snapshot_processes()?;
    Ok(build_process_list(
        raw_processes,
        truncated,
        query_process_metadata,
    ))
}

fn build_process_list(
    mut raw_processes: Vec<ProcessEntry>,
    enumeration_truncated: bool,
    mut metadata_lookup: impl FnMut(u32) -> ProcessMetadata,
) -> ProcessListResult {
    let checked_at = now_rfc3339();
    let mut truncated = enumeration_truncated;

    if raw_processes.len() > MAX_PROCESS_ITEMS {
        raw_processes.truncate(MAX_PROCESS_ITEMS);
        truncated = true;
    }

    let mut truncated_strings = 0usize;
    let parent_lookup: HashMap<u32, String> = raw_processes
        .iter()
        .map(|process| {
            let bounded = bound_text(&process.name, MAX_PROCESS_NAME_CHARS);
            if bounded.truncated {
                truncated_strings += 1;
            }
            (process.pid, bounded.value)
        })
        .collect();

    let mut processes = Vec::with_capacity(raw_processes.len());
    for raw in raw_processes {
        if raw.pid == 0 {
            continue;
        }

        let metadata = metadata_lookup(raw.pid);
        let name = bound_text(&raw.name, MAX_PROCESS_NAME_CHARS);
        if name.truncated {
            truncated_strings += 1;
        }
        let executable_path = metadata.executable_path.map(|path| {
            let bounded = bound_text(&path, MAX_PROCESS_PATH_CHARS);
            if bounded.truncated {
                truncated_strings += 1;
            }
            bounded.value
        });

        processes.push(ProcessSummary {
            pid: raw.pid,
            parent_pid: raw.parent_pid,
            name: name.value,
            executable_path,
            path_access: metadata.path_access,
            parent_name: parent_lookup.get(&raw.parent_pid).cloned(),
            identity: metadata.identity,
            start_time: metadata.start_time,
        });
    }

    processes.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.pid.cmp(&right.pid))
    });

    let mut issues = Vec::new();
    if truncated {
        issues.push(DataIssue {
            code: "processInventoryTruncated".to_string(),
            message: format!(
                "结果不完整：进程快照达到安全上限（{MAX_PROCESS_ITEMS}）；其余进程未收集。"
            ),
            source: "Toolhelp32".to_string(),
        });
    }
    if truncated_strings > 0 {
        truncated = true;
        issues.push(DataIssue {
            code: "processStringTruncated".to_string(),
            message: format!(
                "{truncated_strings} 个进程名称或路径超过显示上限（名称 {MAX_PROCESS_NAME_CHARS} / 路径 {MAX_PROCESS_PATH_CHARS} 字符），已截断。"
            ),
            source: "Toolhelp32".to_string(),
        });
    }

    if processes.is_empty() {
        issues.push(DataIssue {
            code: "processSnapshotEmpty".to_string(),
            message: "进程快照没有返回可显示的用户态进程。".to_string(),
            source: "Toolhelp32".to_string(),
        });
    }

    ProcessListResult {
        checked_at,
        processes,
        issues,
        limitations: vec![
            "这是某一时刻的 Toolhelp32 进程快照，不是实时监控。".to_string(),
            format!("进程快照最多收集 {MAX_PROCESS_ITEMS} 项；达到上限会标记结果不完整。"),
            format!(
                "进程名称最多 {MAX_PROCESS_NAME_CHARS} 字符，路径最多 {MAX_PROCESS_PATH_CHARS} 字符，超限会截断并报告。"
            ),
            "后续检查必须使用快照提供的创建时间身份令牌；仅 PID 不足以标识进程。".to_string(),
            "部分系统进程或受保护进程的路径可能因权限不足而不可读；该情况会逐项标记。".to_string(),
            "创建时间不可读取时 identity 为 null，界面不会发起无法校验身份的可用性检查。"
                .to_string(),
            "PID 0（System Idle Process）不在列表中，因为它没有常规可执行文件。".to_string(),
            "仅覆盖当前可见的用户态进程，不包含内核对象、隐藏对象或完整命令行。".to_string(),
        ],
        truncated,
    }
}

pub fn inspect_process_affinity(
    pid: u32,
    identity_token: &str,
) -> Result<AffinityInspection, CoreError> {
    if pid == 0 {
        return Err(CoreError::InvalidPid);
    }

    let expected_identity =
        ProcessIdentity::parse(identity_token).ok_or(CoreError::InvalidProcessIdentity)?;
    let checked_at = now_rfc3339();

    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    let Some(handle) = OwnedHandle::from_valid(handle) else {
        let error_code = unsafe { GetLastError() };
        return Err(CoreError::ProcessUnavailable {
            pid,
            reason: format!(
                "无法打开进程查询句柄：{}（错误码 {error_code}）",
                io::Error::from_raw_os_error(error_code as i32)
            ),
        });
    };

    let initial_times = read_cpu_times(handle.0).ok_or_else(|| CoreError::ProcessUnavailable {
        pid,
        reason: "无法读取进程创建时间，不能建立稳定身份".to_string(),
    })?;
    let actual_identity =
        ProcessIdentity::from_ticks(initial_times.creation_ticks).ok_or_else(|| {
            CoreError::ProcessUnavailable {
                pid,
                reason: "进程创建时间无效，不能建立稳定身份".to_string(),
            }
        })?;
    if !actual_identity.matches(expected_identity.as_ticks()) {
        return Err(CoreError::ProcessIdentityChanged { pid });
    }

    let (executable_path, path_access) = process_path_from_handle(handle.0);
    let Some(executable_path) = executable_path else {
        return Err(CoreError::ProcessUnavailable {
            pid,
            reason: format!(
                "进程身份已读取，但无法读取可执行路径（{}）",
                path_access_label(path_access)
            ),
        });
    };
    let Some(name) = process_file_name(&executable_path) else {
        return Err(CoreError::ProcessUnavailable {
            pid,
            reason: "可执行文件路径没有可用于展示的文件名".to_string(),
        });
    };

    let start_time = filetime_ticks_to_rfc3339(initial_times.creation_ticks);
    let mut issues = Vec::new();
    let bounded_name = bound_text(&name, MAX_PROCESS_NAME_CHARS);
    let bounded_path = bound_text(&executable_path, MAX_PROCESS_PATH_CHARS);
    let mut display_fields_truncated = bounded_name.truncated || bounded_path.truncated;
    if display_fields_truncated {
        issues.push(DataIssue {
            code: "processStringTruncated".to_string(),
            message: "进程名称或路径超过显示上限，已截断；身份核验和磁盘检查仍使用完整路径。"
                .to_string(),
            source: format!("PID {pid}"),
        });
    }

    let mut signature = verify_file(Path::new(&executable_path));
    if let Some(checked_path) = signature.file_evidence.checked_path.take() {
        let bounded_checked_path = bound_text(&checked_path, MAX_PROCESS_PATH_CHARS);
        if bounded_checked_path.truncated {
            display_fields_truncated = true;
            issues.push(DataIssue {
                code: "processStringTruncated".to_string(),
                message: "磁盘证据路径超过显示上限，已截断。".to_string(),
                source: format!("PID {pid}"),
            });
        }
        signature.file_evidence.checked_path = Some(bounded_checked_path.value);
    }

    let before_sample = read_cpu_times(handle.0).ok_or_else(|| CoreError::ProcessUnavailable {
        pid,
        reason: "读取进程身份进行二次核验失败".to_string(),
    })?;
    if !expected_identity.matches(before_sample.creation_ticks) {
        return Err(CoreError::ProcessIdentityChanged { pid });
    }

    let (affinity_read, affinity_issue) = read_affinity(handle.0);
    if let Some(issue) = affinity_issue {
        issues.push(issue);
    }
    let affinity = affinity_read.as_ref().map(|value| value.result.clone());

    let sample_started = Instant::now();
    std::thread::sleep(CPU_SAMPLE_WINDOW);
    let sample_elapsed = sample_started.elapsed();

    let after_sample = read_cpu_times(handle.0).ok_or_else(|| CoreError::ProcessUnavailable {
        pid,
        reason: "进程在检查结束前退出或身份时间不可读".to_string(),
    })?;
    if !expected_identity.matches(after_sample.creation_ticks) {
        return Err(CoreError::ProcessIdentityChanged { pid });
    }

    let logical_processors = affinity
        .as_ref()
        .map(|value| value.system_mask.count)
        .filter(|count| *count > 0)
        .unwrap_or_else(system_processor_count);

    let process_delta_ticks = after_sample
        .cpu_ticks
        .saturating_sub(before_sample.cpu_ticks);
    let cpu_usage = calculate_cpu_percent(
        ticks_to_duration(process_delta_ticks),
        sample_elapsed,
        logical_processors,
    )
    .map(|percent| CpuUsageSample {
        percent_of_total_capacity: (percent * 100.0).round() / 100.0,
        sample_duration_ms: sample_elapsed.as_millis() as u64,
        sampled_at: now_rfc3339(),
        method: "两次 GetProcessTimes 采样，按当前处理器组可用逻辑处理器容量归一化。".to_string(),
        caveat: "短时采样只代表该时间窗口；高或低占用都不能单独证明正常、恶意或亲和性被修改。"
            .to_string(),
    });

    if cpu_usage.is_none() {
        issues.push(DataIssue {
            code: "cpuSampleUnavailable".to_string(),
            message: "无法计算 CPU 时间样本，占用率保持未知。".to_string(),
            source: format!("PID {pid}"),
        });
    }

    let affinity_risk = assess_affinity(AffinityFacts {
        restricted: affinity_read.as_ref().map(|value| value.result.restricted),
        process_mask: affinity_read.as_ref().map(|value| value.process_mask),
        system_mask: affinity_read.as_ref().map(|value| value.system_mask),
    });
    let overall_risk = assess_process_overall();

    let inspection_state = if !display_fields_truncated
        && affinity.is_some()
        && cpu_usage.is_some()
        && signature.state != crate::model::SignatureState::Unavailable
    {
        InspectionState::Complete
    } else {
        InspectionState::Partial
    };

    Ok(AffinityInspection {
        checked_at,
        pid,
        identity: expected_identity.token(),
        name: bounded_name.value,
        executable_path: bounded_path.value,
        start_time,
        inspection_state,
        signature,
        affinity,
        cpu_usage,
        issues,
        affinity_risk,
        overall_risk,
        limitations: affinity_limitations(),
    })
}

fn process_file_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
}

fn path_access_label(access: ProcessPathAccess) -> &'static str {
    match access {
        ProcessPathAccess::Available => "可读取",
        ProcessPathAccess::Denied => "访问被拒绝",
        ProcessPathAccess::Unavailable => "不可用",
    }
}

struct ProcessSnapshotReader {
    snapshot: OwnedHandle,
    entry: PROCESSENTRY32W,
    first_pending: bool,
    finished: bool,
}

impl ProcessSnapshotReader {
    fn new() -> Result<Self, CoreError> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        let Some(snapshot) = OwnedHandle::from_valid(snapshot) else {
            return Err(CoreError::ProcessSnapshot(
                io::Error::last_os_error().to_string(),
            ));
        };

        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..PROCESSENTRY32W::default()
        };

        if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
            return Err(CoreError::ProcessSnapshot(
                io::Error::last_os_error().to_string(),
            ));
        }

        Ok(Self {
            snapshot,
            entry,
            first_pending: true,
            finished: false,
        })
    }

    fn next_entry(&mut self) -> Option<Result<ProcessEntry, CoreError>> {
        if self.finished {
            return None;
        }

        loop {
            if self.first_pending {
                self.first_pending = false;
            } else if unsafe { Process32NextW(self.snapshot.0, &mut self.entry) } == 0 {
                self.finished = true;
                let error_code = unsafe { GetLastError() };
                if error_code == ERROR_NO_MORE_FILES {
                    return None;
                }
                return Some(Err(CoreError::ProcessSnapshot(
                    io::Error::from_raw_os_error(error_code as i32).to_string(),
                )));
            }

            let name = string_from_wide_until_nul(&self.entry.szExeFile);
            if name.is_empty() {
                continue;
            }

            return Some(Ok(ProcessEntry {
                pid: self.entry.th32ProcessID,
                parent_pid: self.entry.th32ParentProcessID,
                name,
            }));
        }
    }
}

fn collect_bounded_results<T, E>(
    limit: usize,
    mut next: impl FnMut() -> Option<Result<T, E>>,
) -> Result<(Vec<T>, bool), E> {
    let mut items = Vec::with_capacity(limit.min(256));

    while items.len() < limit {
        match next() {
            Some(Ok(item)) => items.push(item),
            Some(Err(error)) => return Err(error),
            None => return Ok((items, false)),
        }
    }

    match next() {
        Some(Ok(_)) => Ok((items, true)),
        Some(Err(error)) => Err(error),
        None => Ok((items, false)),
    }
}

fn snapshot_processes() -> Result<(Vec<ProcessEntry>, bool), CoreError> {
    let mut reader = ProcessSnapshotReader::new()?;
    collect_bounded_results(MAX_PROCESS_ITEMS, || reader.next_entry())
}

fn query_process_metadata(pid: u32) -> ProcessMetadata {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    let Some(handle) = OwnedHandle::from_valid(handle) else {
        let error_code = unsafe { GetLastError() };
        return ProcessMetadata {
            executable_path: None,
            path_access: if error_code == ERROR_ACCESS_DENIED {
                ProcessPathAccess::Denied
            } else {
                ProcessPathAccess::Unavailable
            },
            identity: None,
            start_time: None,
        };
    };

    let (executable_path, path_access) = process_path_from_handle(handle.0);
    let times = read_cpu_times(handle.0);
    let identity = times
        .as_ref()
        .and_then(|value| ProcessIdentity::from_ticks(value.creation_ticks))
        .map(ProcessIdentity::token);
    let start_time = times
        .as_ref()
        .and_then(|value| filetime_ticks_to_rfc3339(value.creation_ticks));

    ProcessMetadata {
        executable_path,
        path_access,
        identity,
        start_time,
    }
}

fn process_path_from_handle(handle: HANDLE) -> (Option<String>, ProcessPathAccess) {
    let mut buffer = vec![0u16; 32_768];
    let mut size = buffer.len() as u32;
    let success = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size)
    };

    if success == 0 {
        let error_code = unsafe { GetLastError() };
        return (
            None,
            if error_code == ERROR_ACCESS_DENIED {
                ProcessPathAccess::Denied
            } else {
                ProcessPathAccess::Unavailable
            },
        );
    }

    let path = string_from_wide_until_nul(&buffer[..size as usize]);
    if path.is_empty() {
        (None, ProcessPathAccess::Unavailable)
    } else {
        (Some(path), ProcessPathAccess::Available)
    }
}

fn read_affinity(handle: HANDLE) -> (Option<AffinityRead>, Option<DataIssue>) {
    let mut process_mask: usize = 0;
    let mut system_mask: usize = 0;
    let success = unsafe { GetProcessAffinityMask(handle, &mut process_mask, &mut system_mask) };
    if success == 0 {
        let error_code = unsafe { GetLastError() };
        return (
            None,
            Some(DataIssue {
                code: "affinityUnavailable".to_string(),
                message: format!(
                    "无法读取处理器亲和性：{}（错误码 {error_code}）。",
                    io::Error::from_raw_os_error(error_code as i32)
                ),
                source: "GetProcessAffinityMask".to_string(),
            }),
        );
    }

    let process_mask = process_mask as u64;
    let system_mask = system_mask as u64;
    let process_cpus = mask_to_cpu_indices(process_mask);
    let system_cpus = mask_to_cpu_indices(system_mask);

    (
        Some(AffinityRead {
            result: AffinityResult {
                process_mask: CpuMask {
                    mask_hex: mask_to_hex(process_mask),
                    count: process_cpus.len() as u32,
                    cpu_indices: process_cpus,
                },
                system_mask: CpuMask {
                    mask_hex: mask_to_hex(system_mask),
                    count: system_cpus.len() as u32,
                    cpu_indices: system_cpus,
                },
                restricted: is_affinity_restricted(process_mask, system_mask),
                comparison_scope: "currentProcessorGroup".to_string(),
                source_trace: AffinitySourceTrace {
                    available: false,
                    explanation:
                        "Windows 当前公开接口不提供 SetProcessAffinityMask 的调用者、时间或持久化来源；不能推断修改者。"
                            .to_string(),
                },
            },
            process_mask,
            system_mask,
        }),
        None,
    )
}

fn read_cpu_times(handle: HANDLE) -> Option<CpuTimes> {
    let mut creation = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut exit = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut kernel = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut user = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };

    let success =
        unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
    if success == 0 {
        return None;
    }

    let creation_ticks = filetime_ticks(creation.dwLowDateTime, creation.dwHighDateTime);
    let kernel_ticks = filetime_ticks(kernel.dwLowDateTime, kernel.dwHighDateTime);
    let user_ticks = filetime_ticks(user.dwLowDateTime, user.dwHighDateTime);

    Some(CpuTimes {
        creation_ticks,
        cpu_ticks: kernel_ticks.saturating_add(user_ticks),
    })
}

fn system_processor_count() -> u32 {
    let mut info = SYSTEM_INFO::default();
    unsafe {
        GetSystemInfo(&mut info);
    }
    info.dwNumberOfProcessors.max(1)
}

fn ticks_to_duration(ticks: u64) -> Duration {
    Duration::new(ticks / 10_000_000, ((ticks % 10_000_000) * 100) as u32)
}

fn affinity_limitations() -> Vec<String> {
    vec![
        "仅比较当前处理器组内的亲和性掩码；超过 64 个逻辑处理器或跨处理器组的细节可能不完整。"
            .to_string(),
        "Windows 公开接口不提供设置亲和性的调用者、时间或持久化来源。".to_string(),
        "本结果只包含已验证创建时间的进程句柄数据；不混入旧 PID 快照的父进程/名称。".to_string(),
        "磁盘文件签名与哈希只对应检查时的本地文件，不检查当前内存映像；不联网、不检查吊销。"
            .to_string(),
        "磁盘文件可在启动后或检查后被替换，因此签名有效 / 哈希一致不等于运行进程绝对可信。"
            .to_string(),
        "网络、设备命名空间和非绝对路径不会进行磁盘签名访问。".to_string(),
        "CPU 使用率是约 500 毫秒的短时采样，按当前处理器组容量归一化，不代表长期负载。".to_string(),
        "本阶段只读：不会结束进程、修改亲和性或更改系统配置。".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    #[test]
    fn invalid_identity_token_is_rejected_before_process_access() {
        let result = inspect_process_affinity(42, "not-a-token");
        assert!(matches!(result, Err(CoreError::InvalidProcessIdentity)));
    }

    #[test]
    fn current_process_rejects_a_mismatched_creation_time_identity() {
        let pid = std::process::id();
        let wrong_identity = ProcessIdentity::from_ticks(1)
            .expect("nonzero ticks")
            .token();
        let result = inspect_process_affinity(pid, &wrong_identity);

        assert!(matches!(
            result,
            Err(CoreError::ProcessIdentityChanged { pid: changed_pid }) if changed_pid == pid
        ));
    }

    #[test]
    fn bounded_process_collection_stops_and_reports_truncation() {
        let mut values: VecDeque<usize> = (0..10).collect();
        let mut calls = 0;

        let (items, truncated) = collect_bounded_results(3, || {
            calls += 1;
            values.pop_front().map(Ok::<usize, ()>)
        })
        .expect("no error");

        assert_eq!(items, vec![0, 1, 2]);
        assert!(truncated);
        assert_eq!(calls, 4, "the enumerator must stop after the probe item");
    }

    #[test]
    fn exact_bound_process_collection_is_not_marked_truncated() {
        let mut values: VecDeque<usize> = (0..3).collect();
        let (items, truncated) =
            collect_bounded_results(3, || values.pop_front().map(Ok::<usize, ()>))
                .expect("no error");

        assert_eq!(items.len(), 3);
        assert!(!truncated);
    }

    #[test]
    fn process_list_builder_marks_inventory_cap_and_issue() {
        let entries = (1..=(MAX_PROCESS_ITEMS + 3))
            .map(|pid| ProcessEntry {
                pid: pid as u32,
                parent_pid: 0,
                name: format!("p{pid}.exe"),
            })
            .collect();

        let result = build_process_list(entries, false, |_| ProcessMetadata {
            executable_path: None,
            path_access: ProcessPathAccess::Unavailable,
            identity: None,
            start_time: None,
        });

        assert_eq!(result.processes.len(), MAX_PROCESS_ITEMS);
        assert!(result.truncated);
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.code == "processInventoryTruncated")
        );
    }

    #[test]
    fn process_list_builder_bounds_names_and_paths_with_visible_issue() {
        let long_name = format!("{}.exe", "n".repeat(MAX_PROCESS_NAME_CHARS + 10));
        let long_path = format!("C:\\{}.exe", "p".repeat(MAX_PROCESS_PATH_CHARS + 10));
        let result = build_process_list(
            vec![ProcessEntry {
                pid: 4242,
                parent_pid: 0,
                name: long_name,
            }],
            false,
            |_| ProcessMetadata {
                executable_path: Some(long_path.clone()),
                path_access: ProcessPathAccess::Available,
                identity: Some("pidct1:00000000000000a1".to_string()),
                start_time: None,
            },
        );

        assert!(result.truncated);
        assert!(result.processes[0].name.chars().count() <= MAX_PROCESS_NAME_CHARS);
        assert!(
            result.processes[0]
                .executable_path
                .as_deref()
                .expect("path")
                .chars()
                .count()
                <= MAX_PROCESS_PATH_CHARS
        );
        assert!(
            result
                .issues
                .iter()
                .any(|issue| issue.code == "processStringTruncated")
        );
    }
}
