use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use winreg::RegValue;
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_BINARY,
    REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ, REG_NONE, REG_QWORD, REG_SZ, RegType,
};
use winreg::types::FromRegValue;
use winreg::{HKEY, RegKey};

use crate::limits::{
    MAX_REGISTRY_STRING_CHARS, MAX_STARTUP_ITEMS_PER_VIEW, MAX_STARTUP_ITEMS_TOTAL,
    MAX_STARTUP_NAME_CHARS, MAX_STARTUP_PATH_CHARS, SourceBudget, bound_text,
};
use crate::model::{
    DataIssue, InstallScope, InventoryObservation, PathState, RegistryView, StartupApprovalState,
    StartupItem, StartupPathResolution, StartupScanResult, StartupSource, StartupSourceStatus,
    StartupTargetKind, StartupValueType,
};
use crate::rules::{StartupFacts, assess_startup};
use crate::shortcut::ShortcutResolver;
use crate::util::{
    ParsedCommandPath, is_local_absolute_path, now_rfc3339, parse_startup_command, stable_token,
};
use crate::win32::{expand_environment, is_access_denied, probe_local_path};

const SCAN_TIME_LIMIT: Duration = Duration::from_secs(8);

struct RunLocation {
    hive: HKEY,
    hive_name: &'static str,
    key_path: &'static str,
    scope: InstallScope,
    view: RegistryView,
    view_flag: u32,
    source: StartupSource,
}

fn registry_locations() -> Vec<RunLocation> {
    let mut locations = Vec::with_capacity(6);
    for (source, key_path) in [
        (
            StartupSource::RegistryRun,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
        ),
        (
            StartupSource::RegistryRunOnce,
            r"Software\Microsoft\Windows\CurrentVersion\RunOnce",
        ),
    ] {
        for (view, view_flag) in [
            (RegistryView::Registry64, KEY_WOW64_64KEY),
            (RegistryView::Registry32, KEY_WOW64_32KEY),
        ] {
            locations.push(RunLocation {
                hive: HKEY_LOCAL_MACHINE,
                hive_name: "HKEY_LOCAL_MACHINE",
                key_path,
                scope: InstallScope::Machine,
                view,
                view_flag,
                source,
            });
        }

        locations.push(RunLocation {
            hive: HKEY_CURRENT_USER,
            hive_name: "HKEY_CURRENT_USER",
            key_path,
            scope: InstallScope::User,
            // HKCU is not split by the same WOW64 views; read it exactly once.
            view: RegistryView::Shared,
            view_flag: 0,
            source,
        });
    }
    locations
}

pub fn scan_startup_items() -> StartupScanResult {
    let started = Instant::now();
    let checked_at = now_rfc3339();
    let mut items = Vec::new();
    let mut issues = Vec::new();
    let mut sources = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut budget = SourceBudget::new(MAX_STARTUP_ITEMS_PER_VIEW, MAX_STARTUP_ITEMS_TOTAL);
    let mut truncated = false;
    let mut time_limited = false;
    let mut truncated_strings = 0usize;
    let mut unquoted_ambiguous = 0usize;
    let mut target_access_denied = 0usize;
    let mut duplicates = 0usize;
    let mut metadata_files_skipped = 0usize;
    let mut partial_items = 0usize;
    let mut item_access_denied = 0usize;
    let mut item_errors = 0usize;
    let mut resolver = ShortcutResolver::new();

    'locations: for location in registry_locations() {
        if deadline_reached(started) {
            mark_time_limited(&mut truncated, &mut time_limited, &mut issues);
            break;
        }

        let root = RegKey::predef(location.hive);
        let full_key = format!("{}\\{}", location.hive_name, location.key_path);
        let source_id = format!(
            "{}:{}:{}",
            location.hive_name,
            registry_view_id(location.view),
            location.key_path
        );
        let mut source_status = StartupSourceStatus {
            id: source_id.clone(),
            source: location.source,
            scope: location.scope,
            registry_view: Some(location.view),
            location: full_key.clone(),
            observation: InventoryObservation::Observed,
            exists: Some(true),
            item_count: 0,
        };

        let key =
            match root.open_subkey_with_flags(location.key_path, KEY_READ | location.view_flag) {
                Ok(key) => key,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    source_status.observation = InventoryObservation::Unavailable;
                    source_status.exists = Some(false);
                    sources.push(source_status);
                    continue;
                }
                Err(error) => {
                    source_status.observation = observation_from_io_error(&error);
                    source_status.exists = None;
                    issues.push(registry_issue(&full_key, &error));
                    sources.push(source_status);
                    continue;
                }
            };

        let items_before = items.len();
        let mut source_partial = false;
        let mut stop_scan = false;
        for entry in key.enum_values() {
            if deadline_reached(started) {
                mark_time_limited(&mut truncated, &mut time_limited, &mut issues);
                source_partial = true;
                stop_scan = true;
                break;
            }
            if budget.total_items() >= MAX_STARTUP_ITEMS_TOTAL {
                truncated = true;
                source_partial = true;
                issues.push(DataIssue {
                    code: "startupTruncated".to_string(),
                    message: format!(
                        "结果不完整：已达到全部启动项的安全扫描上限（{MAX_STARTUP_ITEMS_TOTAL}）。"
                    ),
                    source: full_key.clone(),
                });
                stop_scan = true;
                break;
            }
            if !budget.can_process(&source_id) {
                truncated = true;
                source_partial = true;
                issues.push(DataIssue {
                    code: "startupTruncated".to_string(),
                    message: format!(
                        "结果不完整：已达到每个注册表视图的启动项扫描上限（{MAX_STARTUP_ITEMS_PER_VIEW}）。"
                    ),
                    source: full_key.clone(),
                });
                stop_scan = true;
                break;
            }
            budget.record_processed(&source_id);

            let (raw_value_name, raw_value) = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    source_partial = true;
                    issues.push(registry_issue(&full_key, &error));
                    continue;
                }
            };
            let bounded_name = bound_text(&raw_value_name, MAX_STARTUP_NAME_CHARS);
            if bounded_name.truncated {
                truncated_strings += 1;
                source_partial = true;
            }
            let display_name = bounded_name.value;

            let item = build_registry_item(
                &location,
                &full_key,
                &raw_value_name,
                &display_name,
                bounded_name.truncated,
                &raw_value,
                &mut truncated_strings,
                &mut unquoted_ambiguous,
                &mut target_access_denied,
            );
            if item.observation != InventoryObservation::Observed {
                partial_items += 1;
                source_partial = true;
            }
            if item.observation == InventoryObservation::AccessDenied {
                item_access_denied += 1;
            }
            if item.observation == InventoryObservation::Error {
                item_errors += 1;
            }

            if !seen_ids.insert(item.id.clone()) {
                duplicates += 1;
                continue;
            }

            items.push(item);
            budget.record_item();
        }

        source_status.item_count = items.len() - items_before;
        if source_partial {
            source_status.observation = InventoryObservation::Partial;
        }
        sources.push(source_status);
        if stop_scan {
            break 'locations;
        }
    }

    if deadline_reached(started) && !truncated {
        mark_time_limited(&mut truncated, &mut time_limited, &mut issues);
    }

    for (env_name, scope, source_id, location_display) in [
        (
            "APPDATA",
            InstallScope::User,
            "startup-folder:HKEY_CURRENT_USER",
            r"%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup",
        ),
        (
            "PROGRAMDATA",
            InstallScope::Machine,
            "startup-folder:HKEY_LOCAL_MACHINE",
            r"%PROGRAMDATA%\Microsoft\Windows\Start Menu\Programs\Startup",
        ),
    ] {
        if deadline_reached(started) {
            mark_time_limited(&mut truncated, &mut time_limited, &mut issues);
            break;
        }

        let folder_value =
            format!("%{env_name}%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup");
        let mut source_status = StartupSourceStatus {
            id: source_id.to_string(),
            source: StartupSource::StartupFolder,
            scope,
            registry_view: None,
            location: location_display.to_string(),
            observation: InventoryObservation::Observed,
            exists: Some(true),
            item_count: 0,
        };
        let Some(folder) = expand_environment(&folder_value) else {
            source_status.observation = InventoryObservation::Unavailable;
            source_status.exists = None;
            issues.push(DataIssue {
                code: "startupFolderUnresolved".to_string(),
                message: format!("无法解析 {env_name} 启动文件夹位置。"),
                source: location_display.to_string(),
            });
            sources.push(source_status);
            continue;
        };

        if !is_local_absolute_path(&folder) {
            source_status.observation = InventoryObservation::Unavailable;
            source_status.exists = None;
            issues.push(DataIssue {
                code: "startupFolderRemoteOrRelative".to_string(),
                message:
                    "启动文件夹解析到网络、设备或非绝对路径；为避免访问远程位置，未读取该文件夹。"
                        .to_string(),
                source: location_display.to_string(),
            });
            sources.push(source_status);
            continue;
        }

        let entries = match fs::read_dir(Path::new(&folder)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                source_status.observation = InventoryObservation::Unavailable;
                source_status.exists = Some(false);
                sources.push(source_status);
                continue;
            }
            Err(error) => {
                source_status.observation = observation_from_io_error(&error);
                source_status.exists = None;
                issues.push(DataIssue {
                    code: if is_access_denied(&error) {
                        "startupFolderAccessDenied".to_string()
                    } else {
                        "startupFolderReadFailed".to_string()
                    },
                    message: format!("无法读取启动文件夹“{location_display}”：{error}"),
                    source: location_display.to_string(),
                });
                sources.push(source_status);
                continue;
            }
        };

        let items_before = items.len();
        let mut source_partial = false;
        let mut stop_scan = false;
        for entry in entries {
            if deadline_reached(started) {
                mark_time_limited(&mut truncated, &mut time_limited, &mut issues);
                source_partial = true;
                stop_scan = true;
                break;
            }
            if budget.total_items() >= MAX_STARTUP_ITEMS_TOTAL {
                truncated = true;
                source_partial = true;
                issues.push(DataIssue {
                    code: "startupTruncated".to_string(),
                    message: format!(
                        "结果不完整：已达到全部启动项的安全扫描上限（{MAX_STARTUP_ITEMS_TOTAL}）。"
                    ),
                    source: location_display.to_string(),
                });
                stop_scan = true;
                break;
            }
            if !budget.can_process(source_id) {
                truncated = true;
                source_partial = true;
                issues.push(DataIssue {
                    code: "startupTruncated".to_string(),
                    message: format!(
                        "结果不完整：启动文件夹最多检查 {MAX_STARTUP_ITEMS_PER_VIEW} 项。"
                    ),
                    source: location_display.to_string(),
                });
                stop_scan = true;
                break;
            }
            budget.record_processed(source_id);

            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    source_partial = true;
                    issues.push(DataIssue {
                        code: "startupFolderEntryFailed".to_string(),
                        message: format!("无法读取启动文件夹中的一个条目：{error}"),
                        source: location_display.to_string(),
                    });
                    continue;
                }
            };

            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(error) => {
                    source_partial = true;
                    issues.push(DataIssue {
                        code: "startupFolderEntryFailed".to_string(),
                        message: format!("无法读取启动项文件类型：{error}"),
                        source: location_display.to_string(),
                    });
                    continue;
                }
            };
            if !file_type.is_file() {
                continue;
            }

            let path = entry.path();
            let raw_name = entry.file_name().to_string_lossy().to_string();
            if is_startup_folder_metadata(&raw_name) {
                metadata_files_skipped += 1;
                continue;
            }
            let bounded_name = bound_text(&raw_name, MAX_STARTUP_NAME_CHARS);
            if bounded_name.truncated {
                truncated_strings += 1;
                source_partial = true;
            }

            let item = build_folder_item(
                &path,
                bounded_name.value,
                bounded_name.truncated,
                scope,
                location_display,
                &mut resolver,
                &mut target_access_denied,
            );
            if item.observation != InventoryObservation::Observed {
                partial_items += 1;
                source_partial = true;
            }
            if item.observation == InventoryObservation::AccessDenied {
                item_access_denied += 1;
            }
            if item.observation == InventoryObservation::Error {
                item_errors += 1;
            }

            if !seen_ids.insert(item.id.clone()) {
                duplicates += 1;
                continue;
            }

            items.push(item);
            budget.record_item();
        }

        source_status.item_count = items.len() - items_before;
        if source_partial {
            source_status.observation = InventoryObservation::Partial;
        }
        sources.push(source_status);
        if stop_scan {
            break;
        }
    }

    if truncated_strings > 0 {
        truncated = true;
        issues.push(DataIssue {
            code: "registryStringTruncated".to_string(),
            message: format!(
                "{truncated_strings} 个启动项文本值超过安全长度上限，已截断并以“…”标记。"
            ),
            source: "Startup registry values".to_string(),
        });
    }
    if unquoted_ambiguous > 0 {
        issues.push(DataIssue {
            code: "unquotedPath".to_string(),
            message: format!(
                "{unquoted_ambiguous} 个启动命令包含未加引号且可能含空格的路径；未对其存在性作猜测。"
            ),
            source: "Startup registry commands".to_string(),
        });
    }
    if target_access_denied > 0 {
        issues.push(DataIssue {
            code: "startupTargetAccessDenied".to_string(),
            message: format!(
                "{target_access_denied} 个启动目标访问被拒绝，状态保持未知而不是“不存在”。"
            ),
            source: "Startup targets".to_string(),
        });
    }
    if duplicates > 0 {
        issues.push(DataIssue {
            code: "duplicateStartupEntry".to_string(),
            message: format!("{duplicates} 个重复启动项已被抑制，不重复展示。"),
            source: "Startup inventory".to_string(),
        });
    }
    if metadata_files_skipped > 0 {
        issues.push(DataIssue {
            code: "startupFolderMetadataSkipped".to_string(),
            message: format!(
                "跳过 {metadata_files_skipped} 个 Windows 文件夹元数据文件（如 desktop.ini/Thumbs.db），不计为启动程序。"
            ),
            source: "Startup folders".to_string(),
        });
    }
    if partial_items > 0 {
        issues.push(DataIssue {
            code: "startupPartialItems".to_string(),
            message: format!("{partial_items} 个启动项保留为部分未知；条目证据未被丢弃。"),
            source: "Startup inventory".to_string(),
        });
    }
    if item_access_denied > 0 {
        issues.push(DataIssue {
            code: "startupItemAccessDenied".to_string(),
            message: format!("{item_access_denied} 个启动项因权限不足无法完整读取。"),
            source: "Startup inventory".to_string(),
        });
    }
    if item_errors > 0 {
        issues.push(DataIssue {
            code: "startupItemErrors".to_string(),
            message: format!("{item_errors} 个启动项读取失败；失败没有被当作空值。"),
            source: "Startup inventory".to_string(),
        });
    }

    let observation = aggregate_observation(&sources, partial_items, truncated);
    if items.is_empty() && observation == InventoryObservation::Observed {
        issues.push(DataIssue {
            code: "startupItemsEmpty".to_string(),
            message: "已覆盖位置没有返回启动项；这不代表系统中没有其他启动方式。".to_string(),
            source: "Startup inventory".to_string(),
        });
    }
    if items.is_empty() && observation != InventoryObservation::Observed {
        issues.push(DataIssue {
            code: "startupSourcesUnavailable".to_string(),
            message: "启动项来源未能完整读取；空列表不代表没有启动项。".to_string(),
            source: "Startup inventory".to_string(),
        });
    }

    items.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });

    StartupScanResult {
        checked_at,
        observation,
        sources,
        items,
        issues,
        limitations: vec![
            "仅读取当前用户和本机的 Run、RunOnce，以及两个启动文件夹。".to_string(),
            "HKCU 只读取共享用户视图一次；HKLM Run/RunOnce 分别读取 32/64 位视图（32 位视图对应 WOW6432Node）。"
                .to_string(),
            "未读取计划任务、服务、Winlogon、IFEO、Winlogon Notify、组策略、WMI 订阅、AppInit_DLLs 或浏览器扩展。"
                .to_string(),
            "Windows 启动审批状态未核验；条目存在不保证会被实际执行。".to_string(),
            "启动文件夹中的 .lnk 快捷方式只通过只读 ShellLink 读取目标和“参数/工作目录是否存在”的布尔值；不读取或展示参数文本，不执行快捷方式。"
                .to_string(),
            ".url 快捷方式、MSI 通告式快捷方式和无法由 ShellLink 解析的目标保留为部分未知，不推测目标。"
                .to_string(),
            "未加引号且跨越空格的路径会被标记为歧义，不会探测候选目标或当作缺失文件。"
                .to_string(),
            "路径 Present/Missing/AccessDenied/Unknown 严格区分；网络、设备、相对路径不访问。"
                .to_string(),
            format!(
                "每个注册表视图和每个启动文件夹最多检查 {MAX_STARTUP_ITEMS_PER_VIEW} 项，所有来源合计最多 {MAX_STARTUP_ITEMS_TOTAL} 项；达到上限或 8 秒会标记结果不完整。"
            ),
            format!(
                "注册表值名称最多 {MAX_STARTUP_NAME_CHARS} 字符，值文本最多 {MAX_REGISTRY_STRING_CHARS} 字符；超限会截断并标记，且不会从截断文本猜测路径。"
            ),
            "Windows 文件夹元数据文件（desktop.ini/Thumbs.db）不计为启动程序。".to_string(),
            "未验证启动目标的签名、执行行为、发布者来源或父子进程关系；本版本不会修改、禁用或删除启动项。"
                .to_string(),
        ],
        truncated,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_registry_item(
    location: &RunLocation,
    full_key: &str,
    raw_value_name: &str,
    display_name: &str,
    name_truncated: bool,
    raw_value: &RegValue<'_>,
    truncated_strings: &mut usize,
    unquoted_ambiguous: &mut usize,
    target_access_denied: &mut usize,
) -> StartupItem {
    let value_type = Some(map_value_type(raw_value.vtype.clone()));
    let mut issues = Vec::new();
    let mut observation = InventoryObservation::Observed;
    let mut raw_value_readable = false;
    let mut target_path = None;
    let mut target_candidate = None;
    let mut target_state = PathState::Unknown;
    let mut path_resolution = StartupPathResolution::Unresolved;

    if name_truncated {
        observation = InventoryObservation::Partial;
        issues.push(DataIssue {
            code: "STARTUP-NAME-TRUNCATED".to_string(),
            message: format!("值名称超过 {MAX_STARTUP_NAME_CHARS} 字符上限，已截断显示。"),
            source: full_key.to_string(),
        });
    }

    match decode_registry_value(raw_value) {
        RegistryValueDecode::Command(raw_command) => {
            raw_value_readable = true;
            let bounded = bound_text(&raw_command, MAX_REGISTRY_STRING_CHARS);
            if bounded.truncated {
                *truncated_strings += 1;
                observation = InventoryObservation::Partial;
                issues.push(DataIssue {
                    code: "STARTUP-VALUE-TRUNCATED".to_string(),
                    message: format!(
                        "注册表值超过 {MAX_REGISTRY_STRING_CHARS} 字符上限；为避免猜测，未解析目标路径。"
                    ),
                    source: full_key.to_string(),
                });
            } else {
                let resolved = resolve_startup_target(&bounded.value);
                target_path = resolved.target_path;
                target_candidate = resolved.target_candidate;
                target_state = resolved.target_state;
                path_resolution = resolved.path_resolution;
                if resolved.truncated {
                    observation = InventoryObservation::Partial;
                    issues.push(DataIssue {
                        code: "STARTUP-PATH-TRUNCATED".to_string(),
                        message: format!(
                            "目标路径超过 {MAX_STARTUP_PATH_CHARS} 字符上限，已截断显示。"
                        ),
                        source: full_key.to_string(),
                    });
                }
                if path_resolution == StartupPathResolution::AmbiguousUnquoted {
                    *unquoted_ambiguous += 1;
                    issues.push(DataIssue {
                        code: "STARTUP-PATH-AMBIGUOUS".to_string(),
                        message: "值未被完整引号包围，无法安全确定可执行文件边界；未探测候选路径。"
                            .to_string(),
                        source: full_key.to_string(),
                    });
                }
                if path_resolution == StartupPathResolution::Unresolved {
                    observation = InventoryObservation::Partial;
                    issues.push(DataIssue {
                        code: "STARTUP-PATH-UNRESOLVED".to_string(),
                        message: "注册表值不是可安全解析的命令；目标路径保持未知。".to_string(),
                        source: full_key.to_string(),
                    });
                }
            }
        }
        RegistryValueDecode::Empty => {
            raw_value_readable = true;
            observation = InventoryObservation::Partial;
            issues.push(DataIssue {
                code: "STARTUP-VALUE-EMPTY".to_string(),
                message: "注册表值为空；条目保留，目标路径未知。".to_string(),
                source: full_key.to_string(),
            });
        }
        RegistryValueDecode::Unsupported => {
            observation = InventoryObservation::Partial;
            issues.push(DataIssue {
                code: "STARTUP-VALUE-TYPE-UNSUPPORTED".to_string(),
                message: format!(
                    "注册表值类型 {value_type:?} 不是可解析的启动命令；条目保留，目标未推测。"
                ),
                source: full_key.to_string(),
            });
        }
        RegistryValueDecode::Invalid => {
            observation = InventoryObservation::Error;
            raw_value_readable = false;
            issues.push(DataIssue {
                code: "STARTUP-VALUE-INVALID".to_string(),
                message: "注册表值无法解码为文本；条目保留，目标保持未知。".to_string(),
                source: full_key.to_string(),
            });
        }
    }

    if target_state == PathState::AccessDenied {
        *target_access_denied += 1;
    }

    let risk = assess_startup(StartupFacts {
        target_path: target_path.as_deref(),
        target_state,
        path_resolution,
    });

    StartupItem {
        id: startup_registry_id(location.scope, location.view, full_key, raw_value_name),
        name: display_name.to_string(),
        value_name: Some(display_name.to_string()),
        value_type,
        raw_value_readable,
        source: location.source,
        scope: location.scope,
        registry_view: Some(location.view),
        registry_key: Some(full_key.to_string()),
        target_kind: StartupTargetKind::RegistryCommand,
        target_path,
        target_candidate,
        target_state,
        path_resolution,
        shortcut_arguments_present: None,
        shortcut_working_directory_present: None,
        approval_state: StartupApprovalState::NotChecked,
        observation,
        issues,
        risk,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_folder_item(
    path: &Path,
    display_name: String,
    name_truncated: bool,
    scope: InstallScope,
    location_display: &str,
    resolver: &mut ShortcutResolver,
    target_access_denied: &mut usize,
) -> StartupItem {
    let command_full = path.display().to_string();
    let command_bounded = bound_text(&command_full, MAX_STARTUP_PATH_CHARS);
    let command = command_bounded.value;
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase());
    let is_lnk = extension.as_deref() == Some("lnk");
    let is_url = extension.as_deref() == Some("url");
    let mut issues = Vec::new();
    let mut observation = InventoryObservation::Observed;
    let mut target_kind = StartupTargetKind::StartupFolderFile;
    let mut target_path = Some(command.clone());
    let mut target_state = PathState::Present;
    let mut path_resolution = StartupPathResolution::StartupFolderFile;
    let mut shortcut_arguments_present = None;
    let mut shortcut_working_directory_present = None;

    if command_bounded.truncated {
        observation = InventoryObservation::Partial;
        issues.push(DataIssue {
            code: "STARTUP-PATH-TRUNCATED".to_string(),
            message: format!("启动项路径超过 {MAX_STARTUP_PATH_CHARS} 字符上限，已截断显示。"),
            source: location_display.to_string(),
        });
    }

    if name_truncated {
        observation = InventoryObservation::Partial;
        issues.push(DataIssue {
            code: "STARTUP-NAME-TRUNCATED".to_string(),
            message: format!("文件名超过 {MAX_STARTUP_NAME_CHARS} 字符上限，已截断显示。"),
            source: location_display.to_string(),
        });
    }

    if is_lnk {
        target_kind = StartupTargetKind::Shortcut;
        let shortcut = resolver.resolve(path);
        observation = merge_observation(observation, shortcut.observation);
        shortcut_arguments_present = shortcut.arguments_present;
        shortcut_working_directory_present = shortcut.working_directory_present;
        target_path = shortcut.target_path;
        target_state = if shortcut.target_truncated {
            PathState::Unknown
        } else {
            target_path
                .as_deref()
                .map(|target| probe_local_path(target).1)
                .unwrap_or(PathState::Unknown)
        };
        path_resolution =
            if target_path.is_some() && shortcut.observation == InventoryObservation::Observed {
                StartupPathResolution::ShortcutTargetResolved
            } else {
                StartupPathResolution::ShortcutTargetUnresolved
            };
        for issue in shortcut.issues {
            issues.push(DataIssue {
                code: issue.code.to_string(),
                message: issue.message,
                source: format!("{location_display}\\{display_name}"),
            });
        }
        if path_resolution == StartupPathResolution::ShortcutTargetUnresolved
            && observation == InventoryObservation::Observed
        {
            observation = InventoryObservation::Partial;
            issues.push(DataIssue {
                code: "STARTUP-SHORTCUT-TARGET-UNRESOLVED".to_string(),
                message: "快捷方式目标无法解析；条目保留为部分未知。".to_string(),
                source: format!("{location_display}\\{display_name}"),
            });
        }
    } else if is_url {
        target_kind = StartupTargetKind::Shortcut;
        target_path = None;
        target_state = PathState::Unknown;
        path_resolution = StartupPathResolution::ShortcutTargetUnresolved;
        observation = merge_observation(observation, InventoryObservation::Partial);
        issues.push(DataIssue {
            code: "STARTUP-URL-NOT-RESOLVED".to_string(),
            message: "Internet 快捷方式（.url）未解析目标；条目保留为部分未知。".to_string(),
            source: format!("{location_display}\\{display_name}"),
        });
    }

    if target_state == PathState::AccessDenied {
        *target_access_denied += 1;
    }

    let risk = assess_startup(StartupFacts {
        target_path: target_path.as_deref(),
        target_state,
        path_resolution,
    });

    StartupItem {
        id: format!(
            "startup:folder:{}:{}",
            scope_id(scope),
            stable_token(&command_full)
        ),
        name: display_name,
        value_name: None,
        value_type: None,
        raw_value_readable: true,
        source: StartupSource::StartupFolder,
        scope,
        registry_view: None,
        registry_key: None,
        target_kind,
        target_path,
        target_candidate: None,
        target_state,
        path_resolution,
        shortcut_arguments_present,
        shortcut_working_directory_present,
        approval_state: StartupApprovalState::NotChecked,
        observation,
        issues,
        risk,
    }
}

struct ResolvedStartupPath {
    target_path: Option<String>,
    target_candidate: Option<String>,
    target_state: PathState,
    path_resolution: StartupPathResolution,
    truncated: bool,
}

fn resolve_startup_target(command: &str) -> ResolvedStartupPath {
    match parse_startup_command(command) {
        ParsedCommandPath::Quoted(path) => {
            let (resolved, state) = probe_local_path(&path);
            let bounded = resolved
                .or(Some(path))
                .map(|value| bound_text(&value, MAX_STARTUP_PATH_CHARS));
            let truncated = bounded.as_ref().is_some_and(|value| value.truncated);
            ResolvedStartupPath {
                target_path: bounded.map(|value| value.value),
                target_candidate: None,
                target_state: state,
                path_resolution: StartupPathResolution::QuotedExecutable,
                truncated,
            }
        }
        ParsedCommandPath::Unquoted(path) => {
            let (resolved, state) = probe_local_path(&path);
            let bounded = resolved
                .or(Some(path))
                .map(|value| bound_text(&value, MAX_STARTUP_PATH_CHARS));
            let truncated = bounded.as_ref().is_some_and(|value| value.truncated);
            ResolvedStartupPath {
                target_path: bounded.map(|value| value.value),
                target_candidate: None,
                target_state: state,
                path_resolution: StartupPathResolution::UnquotedExecutable,
                truncated,
            }
        }
        ParsedCommandPath::AmbiguousUnquoted { candidate } => {
            let bounded_candidate =
                candidate.map(|value| bound_text(&value, MAX_STARTUP_PATH_CHARS));
            let truncated = bounded_candidate
                .as_ref()
                .is_some_and(|value| value.truncated);
            ResolvedStartupPath {
                target_path: None,
                target_candidate: bounded_candidate.map(|value| value.value),
                target_state: PathState::Unknown,
                path_resolution: StartupPathResolution::AmbiguousUnquoted,
                truncated,
            }
        }
        ParsedCommandPath::Unresolved => ResolvedStartupPath {
            target_path: None,
            target_candidate: None,
            target_state: PathState::Unknown,
            path_resolution: StartupPathResolution::Unresolved,
            truncated: false,
        },
    }
}

enum RegistryValueDecode {
    Command(String),
    Empty,
    Unsupported,
    Invalid,
}

fn decode_registry_value(raw_value: &RegValue<'_>) -> RegistryValueDecode {
    match raw_value.vtype.clone() {
        REG_SZ | REG_EXPAND_SZ => match String::from_reg_value(raw_value) {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    RegistryValueDecode::Empty
                } else {
                    RegistryValueDecode::Command(trimmed.to_string())
                }
            }
            Err(_) => RegistryValueDecode::Invalid,
        },
        _ => RegistryValueDecode::Unsupported,
    }
}

fn map_value_type(value_type: RegType) -> StartupValueType {
    match value_type {
        REG_SZ => StartupValueType::RegSz,
        REG_EXPAND_SZ => StartupValueType::RegExpandSz,
        REG_MULTI_SZ => StartupValueType::RegMultiSz,
        REG_DWORD => StartupValueType::RegDword,
        REG_QWORD => StartupValueType::RegQword,
        REG_BINARY => StartupValueType::RegBinary,
        REG_NONE => StartupValueType::RegNone,
        _ => StartupValueType::Other,
    }
}

fn deadline_reached(started: Instant) -> bool {
    started.elapsed() >= SCAN_TIME_LIMIT
}

fn mark_time_limited(truncated: &mut bool, time_limited: &mut bool, issues: &mut Vec<DataIssue>) {
    *truncated = true;
    if !*time_limited {
        *time_limited = true;
        issues.push(DataIssue {
            code: "startupTimeLimit".to_string(),
            message: "启动项盘点超过 8 秒安全上限，已停止继续读取；结果可能不完整。".to_string(),
            source: "Startup inventory".to_string(),
        });
    }
}

fn observation_from_io_error(error: &std::io::Error) -> InventoryObservation {
    if is_access_denied(error) {
        InventoryObservation::AccessDenied
    } else if error.kind() == std::io::ErrorKind::NotFound {
        InventoryObservation::Unavailable
    } else {
        InventoryObservation::Error
    }
}

fn merge_observation(
    current: InventoryObservation,
    next: InventoryObservation,
) -> InventoryObservation {
    if current == InventoryObservation::Error || next == InventoryObservation::Error {
        InventoryObservation::Error
    } else if current == InventoryObservation::AccessDenied
        || next == InventoryObservation::AccessDenied
    {
        InventoryObservation::AccessDenied
    } else if current == InventoryObservation::Partial
        || next == InventoryObservation::Partial
        || current == InventoryObservation::Unavailable
        || next == InventoryObservation::Unavailable
    {
        InventoryObservation::Partial
    } else {
        InventoryObservation::Observed
    }
}

fn aggregate_observation(
    sources: &[StartupSourceStatus],
    partial_items: usize,
    truncated: bool,
) -> InventoryObservation {
    let readable = sources.iter().any(|source| {
        matches!(
            source.observation,
            InventoryObservation::Observed | InventoryObservation::Partial
        )
    });
    let any_access_denied = sources
        .iter()
        .any(|source| source.observation == InventoryObservation::AccessDenied);
    let any_error = sources
        .iter()
        .any(|source| source.observation == InventoryObservation::Error);
    let any_unresolved = sources.iter().any(|source| {
        source.observation == InventoryObservation::Unavailable && source.exists != Some(false)
    });
    let any_partial = sources
        .iter()
        .any(|source| source.observation == InventoryObservation::Partial);

    if !readable {
        if any_access_denied {
            return InventoryObservation::AccessDenied;
        }
        if any_error {
            return InventoryObservation::Error;
        }
        return InventoryObservation::Unavailable;
    }

    if truncated
        || partial_items > 0
        || any_partial
        || any_access_denied
        || any_error
        || any_unresolved
    {
        InventoryObservation::Partial
    } else {
        InventoryObservation::Observed
    }
}

fn startup_registry_id(
    scope: InstallScope,
    view: RegistryView,
    full_key: &str,
    value_name: &str,
) -> String {
    let identity = format!("{full_key}\u{0}{value_name}");
    let token = stable_token(&identity);
    match scope {
        InstallScope::User => format!("startup:user:{token}"),
        InstallScope::Machine => format!("startup:machine:{}:{token}", registry_view_id(view)),
    }
}

pub fn is_startup_folder_metadata(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "desktop.ini" | "thumbs.db"
    )
}

fn scope_id(scope: InstallScope) -> &'static str {
    match scope {
        InstallScope::Machine => "machine",
        InstallScope::User => "user",
    }
}

fn registry_view_id(view: RegistryView) -> &'static str {
    match view {
        RegistryView::Registry32 => "registry32",
        RegistryView::Registry64 => "registry64",
        RegistryView::Shared => "shared",
    }
}

fn registry_issue(source: &str, error: &std::io::Error) -> DataIssue {
    DataIssue {
        code: if is_access_denied(error) {
            "startupRegistryAccessDenied".to_string()
        } else {
            "startupRegistryReadFailed".to_string()
        },
        message: format!("无法读取启动项注册表位置：{error}"),
        source: source.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    fn registry_value(value_type: RegType, text: &str) -> RegValue<'static> {
        let mut bytes = Vec::new();
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        RegValue {
            bytes: Cow::Owned(bytes),
            vtype: value_type,
        }
    }

    fn source(observation: InventoryObservation, exists: Option<bool>) -> StartupSourceStatus {
        StartupSourceStatus {
            id: "test".to_string(),
            source: StartupSource::RegistryRun,
            scope: InstallScope::User,
            registry_view: Some(RegistryView::Shared),
            location: "HKEY_CURRENT_USER\\...\\Run".to_string(),
            observation,
            exists,
            item_count: 0,
        }
    }

    #[test]
    fn hkcu_has_one_shared_location_for_each_startup_source() {
        let user_locations: Vec<_> = registry_locations()
            .into_iter()
            .filter(|location| location.scope == InstallScope::User)
            .collect();

        assert_eq!(user_locations.len(), 2);
        assert!(
            user_locations
                .iter()
                .all(|location| location.view == RegistryView::Shared && location.view_flag == 0)
        );

        let mut key_paths: Vec<_> = user_locations
            .iter()
            .map(|location| location.key_path)
            .collect();
        key_paths.sort_unstable();
        key_paths.dedup();
        assert_eq!(
            key_paths.len(),
            user_locations.len(),
            "HKCU Run and RunOnce must each be scanned exactly once"
        );
    }

    #[test]
    fn hkcu_startup_identity_has_no_view_selector() {
        let id = startup_registry_id(
            InstallScope::User,
            RegistryView::Shared,
            r"HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run",
            "Example",
        );

        assert!(!id.contains("registry32"));
        assert!(!id.contains("registry64"));
    }

    #[test]
    fn hklm_startup_views_remain_distinct() {
        let machine_locations: Vec<_> = registry_locations()
            .into_iter()
            .filter(|location| location.scope == InstallScope::Machine)
            .collect();
        assert_eq!(machine_locations.len(), 4);
    }

    #[test]
    fn registry_value_types_are_preserved() {
        assert_eq!(map_value_type(REG_SZ), StartupValueType::RegSz);
        assert_eq!(map_value_type(REG_EXPAND_SZ), StartupValueType::RegExpandSz);
        assert_eq!(map_value_type(REG_MULTI_SZ), StartupValueType::RegMultiSz);
        assert_eq!(map_value_type(REG_DWORD), StartupValueType::RegDword);
        assert_eq!(map_value_type(REG_QWORD), StartupValueType::RegQword);
        assert_eq!(map_value_type(REG_BINARY), StartupValueType::RegBinary);
        assert_eq!(map_value_type(REG_NONE), StartupValueType::RegNone);
    }

    #[test]
    fn string_values_decode_and_other_types_do_not_guess() {
        match decode_registry_value(&registry_value(REG_SZ, r#""C:\Example\app.exe" --run"#)) {
            RegistryValueDecode::Command(command) => {
                assert_eq!(command, r#""C:\Example\app.exe" --run"#);
            }
            _ => panic!("expected a decoded command"),
        }

        assert!(matches!(
            decode_registry_value(&registry_value(REG_SZ, "   ")),
            RegistryValueDecode::Empty
        ));
        assert!(matches!(
            decode_registry_value(&registry_value(REG_DWORD, "")),
            RegistryValueDecode::Unsupported
        ));
    }

    #[test]
    fn absent_sources_do_not_turn_an_observed_inventory_into_partial() {
        let sources = vec![
            source(InventoryObservation::Observed, Some(true)),
            source(InventoryObservation::Unavailable, Some(false)),
        ];
        assert_eq!(
            aggregate_observation(&sources, 0, false),
            InventoryObservation::Observed
        );
    }

    #[test]
    fn long_command_paths_are_bounded_before_entering_the_result() {
        let long_path = format!(r#""C:\{}""#, "a".repeat(MAX_STARTUP_PATH_CHARS + 64));
        let resolved = resolve_startup_target(&long_path);

        assert!(resolved.truncated);
        assert!(
            resolved
                .target_path
                .as_deref()
                .is_some_and(|path| path.chars().count() <= MAX_STARTUP_PATH_CHARS)
        );
    }

    #[test]
    fn prefix_issues_are_not_overwritten_by_a_successful_shortcut_read() {
        assert_eq!(
            merge_observation(
                InventoryObservation::Partial,
                InventoryObservation::Observed
            ),
            InventoryObservation::Partial
        );
        assert_eq!(
            merge_observation(
                InventoryObservation::Partial,
                InventoryObservation::AccessDenied
            ),
            InventoryObservation::AccessDenied
        );
        assert_eq!(
            merge_observation(
                InventoryObservation::Observed,
                InventoryObservation::Observed
            ),
            InventoryObservation::Observed
        );
    }

    #[test]
    fn missing_and_access_denied_sources_are_distinct() {
        let missing = std::io::Error::from(std::io::ErrorKind::NotFound);
        let denied = std::io::Error::from_raw_os_error(5);
        let other = std::io::Error::from_raw_os_error(1117);

        assert_eq!(
            observation_from_io_error(&missing),
            InventoryObservation::Unavailable
        );
        assert_eq!(
            observation_from_io_error(&denied),
            InventoryObservation::AccessDenied
        );
        assert_eq!(
            observation_from_io_error(&other),
            InventoryObservation::Error
        );
    }

    #[test]
    fn denied_or_partial_sources_are_not_normalized_to_observed() {
        let sources = vec![
            source(InventoryObservation::Observed, Some(true)),
            source(InventoryObservation::AccessDenied, None),
        ];
        assert_eq!(
            aggregate_observation(&sources, 0, false),
            InventoryObservation::Partial
        );

        let sources = vec![source(InventoryObservation::AccessDenied, None)];
        assert_eq!(
            aggregate_observation(&sources, 0, false),
            InventoryObservation::AccessDenied
        );

        let sources = vec![source(InventoryObservation::Error, None)];
        assert_eq!(
            aggregate_observation(&sources, 0, false),
            InventoryObservation::Error
        );

        let sources = vec![source(InventoryObservation::Unavailable, None)];
        assert_eq!(
            aggregate_observation(&sources, 0, false),
            InventoryObservation::Unavailable
        );
    }
}
