use std::collections::HashSet;
use std::time::{Duration, Instant};

use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_DWORD,
    REG_EXPAND_SZ, REG_SZ, RegType,
};
use winreg::{HKEY, RegKey};

use crate::limits::{
    MAX_REGISTRY_STRING_CHARS, MAX_SOFTWARE_ICON_PATH_CHARS, MAX_SOFTWARE_ITEMS_PER_VIEW,
    MAX_SOFTWARE_ITEMS_TOTAL, SourceBudget, bound_text,
};
use crate::model::{
    DataIssue, InstallScope, InstalledSoftware, InventoryObservation, PathState,
    RegistryValueState, RegistryView, SoftwareClassification, SoftwareScanResult,
    SoftwareSourceStatus,
};
use crate::registry::{ParsedRegistryValue, RegistryValueKind, parse_dword, parse_string};
use crate::rules::{SoftwareFacts, assess_software};
use crate::util::{normalize_install_date, now_rfc3339, stable_token};
use crate::win32::{is_access_denied, probe_local_path};

const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const SOFTWARE_SCAN_TIME_LIMIT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy)]
pub struct SoftwareClassificationFacts<'a> {
    pub system_component: Option<u32>,
    pub no_remove: Option<u32>,
    pub parent_key_name: Option<&'a str>,
    pub release_type: Option<&'a str>,
    pub system_component_state: RegistryValueState,
    pub no_remove_state: RegistryValueState,
    pub parent_key_name_state: RegistryValueState,
    pub release_type_state: RegistryValueState,
}

/// Classifies an Uninstall entry from registry semantics without brand-name hardcoding.
///
/// Ambiguous component/update metadata is deliberately classified as `Unknown` rather than
/// guessing "driver" or "runtime".
pub fn classify_software(facts: SoftwareClassificationFacts<'_>) -> SoftwareClassification {
    if facts.system_component.is_some_and(|value| value != 0)
        || facts.no_remove.is_some_and(|value| value != 0)
    {
        return SoftwareClassification::SystemComponent;
    }

    if is_unreadable(facts.system_component_state)
        || is_unreadable(facts.no_remove_state)
        || is_unreadable(facts.parent_key_name_state)
        || is_unreadable(facts.release_type_state)
    {
        return SoftwareClassification::Unknown;
    }

    let release_type = facts
        .release_type
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if release_type.is_some_and(is_driver_or_hardware_release) {
        return SoftwareClassification::DriverOrHardwareComponent;
    }

    let has_parent = facts
        .parent_key_name
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    if has_parent || release_type.is_some() {
        return SoftwareClassification::Unknown;
    }

    SoftwareClassification::UserSoftware
}

fn is_unreadable(state: RegistryValueState) -> bool {
    matches!(
        state,
        RegistryValueState::TypeMismatch
            | RegistryValueState::Invalid
            | RegistryValueState::ReadError
    )
}

fn is_driver_or_hardware_release(release_type: &str) -> bool {
    let lower = release_type.to_ascii_lowercase();
    ["driver", "firmware", "hardware"]
        .iter()
        .any(|needle| lower.contains(needle))
}

struct UninstallLocation {
    hive: HKEY,
    hive_name: &'static str,
    scope: InstallScope,
    view: RegistryView,
    view_flag: u32,
}

const LOCATIONS: [UninstallLocation; 3] = [
    UninstallLocation {
        hive: HKEY_LOCAL_MACHINE,
        hive_name: "HKEY_LOCAL_MACHINE",
        scope: InstallScope::Machine,
        view: RegistryView::Registry64,
        view_flag: KEY_WOW64_64KEY,
    },
    UninstallLocation {
        hive: HKEY_LOCAL_MACHINE,
        hive_name: "HKEY_LOCAL_MACHINE",
        scope: InstallScope::Machine,
        view: RegistryView::Registry32,
        view_flag: KEY_WOW64_32KEY,
    },
    UninstallLocation {
        hive: HKEY_CURRENT_USER,
        hive_name: "HKEY_CURRENT_USER",
        scope: InstallScope::User,
        // HKCU does not expose the same WOW64 32/64 split. Reading both flags would duplicate
        // the same user keys, so this view is read once without a WOW64 selector.
        view: RegistryView::Shared,
        view_flag: 0,
    },
];

pub fn scan_installed_software() -> SoftwareScanResult {
    let started = Instant::now();
    let checked_at = now_rfc3339();
    let mut items = Vec::new();
    let mut issues = Vec::new();
    let mut sources = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut budget = SourceBudget::new(MAX_SOFTWARE_ITEMS_PER_VIEW, MAX_SOFTWARE_ITEMS_TOTAL);
    let mut truncated = false;
    let mut time_limited = false;
    let mut truncated_strings = 0usize;
    let mut access_denied_paths = 0usize;
    let mut duplicate_entries = 0usize;
    let mut entries_without_display_name = 0usize;
    let mut display_name_unreadable = 0usize;
    let mut partial_items = 0usize;

    'locations: for location in LOCATIONS {
        if deadline_reached(started) {
            mark_time_limited(&mut truncated, &mut time_limited, &mut issues);
            break;
        }

        let root = RegKey::predef(location.hive);
        let source_id = source_id(&location);
        let full_key = format!("{}\\{}", location.hive_name, UNINSTALL_KEY);
        let mut source_status = SoftwareSourceStatus {
            id: source_id.clone(),
            scope: location.scope,
            registry_view: location.view,
            registry_key: full_key.clone(),
            observation: InventoryObservation::Observed,
            exists: Some(true),
            item_count: 0,
        };
        let uninstall_root =
            match root.open_subkey_with_flags(UNINSTALL_KEY, KEY_READ | location.view_flag) {
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
                    issues.push(registry_issue(
                        &full_key,
                        &error,
                        "无法打开卸载注册表位置，部分软件可能缺失。",
                    ));
                    sources.push(source_status);
                    continue;
                }
            };

        let items_before = items.len();
        let mut source_partial = false;
        let mut stop_scan = false;
        for subkey_name in uninstall_root.enum_keys() {
            if deadline_reached(started) {
                mark_time_limited(&mut truncated, &mut time_limited, &mut issues);
                source_partial = true;
                stop_scan = true;
                break;
            }
            if budget.total_items() >= MAX_SOFTWARE_ITEMS_TOTAL {
                truncated = true;
                source_partial = true;
                issues.push(DataIssue {
                    code: "inventoryTruncated".to_string(),
                    message: format!(
                        "结果不完整：已达到全部软件条目的安全扫描上限（{MAX_SOFTWARE_ITEMS_TOTAL}）。"
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
                    code: "inventoryTruncated".to_string(),
                    message: format!(
                        "结果不完整：已达到每个注册表视图的安全扫描上限（{MAX_SOFTWARE_ITEMS_PER_VIEW}）。"
                    ),
                    source: full_key.clone(),
                });
                stop_scan = true;
                break;
            }
            budget.record_processed(&source_id);

            let subkey_name = match subkey_name {
                Ok(name) => name,
                Err(error) => {
                    source_partial = true;
                    issues.push(registry_issue(
                        &full_key,
                        &error,
                        "无法枚举卸载注册表子项。",
                    ));
                    continue;
                }
            };

            let subkey = match uninstall_root
                .open_subkey_with_flags(&subkey_name, KEY_READ | location.view_flag)
            {
                Ok(key) => key,
                Err(error) => {
                    source_partial = true;
                    let issue_source = bound_text(
                        &format!("{full_key}\\{subkey_name}"),
                        MAX_REGISTRY_STRING_CHARS,
                    );
                    if issue_source.truncated {
                        truncated_strings += 1;
                    }
                    issues.push(registry_issue(
                        &issue_source.value,
                        &error,
                        "无法读取一个软件条目，已跳过。",
                    ));
                    continue;
                }
            };

            let hive_and_key = format!("{full_key}\\{subkey_name}");
            let bounded_key = bound_text(&hive_and_key, MAX_REGISTRY_STRING_CHARS);
            if bounded_key.truncated {
                truncated_strings += 1;
            }

            let display_name_field =
                read_string_field(&subkey, "DisplayName", &mut truncated_strings);
            let Some(display_name) = non_empty(display_name_field.value) else {
                if is_unreadable(display_name_field.state) {
                    display_name_unreadable += 1;
                    source_partial = true;
                } else {
                    entries_without_display_name += 1;
                }
                continue;
            };

            let version_field =
                read_string_field(&subkey, "DisplayVersion", &mut truncated_strings);
            let publisher_field = read_string_field(&subkey, "Publisher", &mut truncated_strings);
            let install_location_field =
                read_string_field(&subkey, "InstallLocation", &mut truncated_strings);
            let install_location = non_empty(install_location_field.value);
            let install_location_state = install_location
                .as_deref()
                .map(probe_local_path)
                .map(|(_, state)| state)
                .unwrap_or(PathState::Unknown);
            if install_location_state == PathState::AccessDenied {
                access_denied_paths += 1;
            }

            let install_date_field =
                read_string_field(&subkey, "InstallDate", &mut truncated_strings);
            let (install_date, install_date_state) = normalized_install_date(&install_date_field);

            let display_icon_field =
                read_string_field(&subkey, "DisplayIcon", &mut truncated_strings);
            let (display_icon_path, display_icon_path_truncated) = display_icon_field
                .value
                .as_deref()
                .and_then(parse_display_icon_path)
                .map(|path| {
                    let bounded = bound_text(&path, MAX_SOFTWARE_ICON_PATH_CHARS);
                    (Some(bounded.value), bounded.truncated)
                })
                .unwrap_or((None, false));

            let uninstall_field =
                read_string_field(&subkey, "UninstallString", &mut truncated_strings);
            let uninstall_command_present = uninstall_field
                .value
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
            let quiet_uninstall_field =
                read_string_field(&subkey, "QuietUninstallString", &mut truncated_strings);
            let quiet_uninstall_command_present = quiet_uninstall_field
                .value
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());

            let windows_installer = read_dword_field(&subkey, "WindowsInstaller");
            let system_component = read_classification_dword(
                &subkey,
                "SystemComponent",
                &bounded_key.value,
                &mut issues,
            );
            let no_remove =
                read_classification_dword(&subkey, "NoRemove", &bounded_key.value, &mut issues);
            let parent_key_name = read_classification_string(
                &subkey,
                "ParentKeyName",
                &bounded_key.value,
                &mut truncated_strings,
                &mut issues,
            );
            let release_type = read_classification_string(
                &subkey,
                "ReleaseType",
                &bounded_key.value,
                &mut truncated_strings,
                &mut issues,
            );

            let classification = classify_software(SoftwareClassificationFacts {
                system_component: system_component.value,
                no_remove: no_remove.value,
                parent_key_name: parent_key_name.value.as_deref(),
                release_type: release_type.value.as_deref(),
                system_component_state: system_component.state,
                no_remove_state: no_remove.state,
                parent_key_name_state: parent_key_name.state,
                release_type_state: release_type.state,
            });

            let mut item_issues = Vec::new();
            let mut item_truncated = display_name_field.truncated
                || version_field.truncated
                || publisher_field.truncated
                || install_location_field.truncated
                || install_date_field.truncated
                || display_icon_field.truncated
                || uninstall_field.truncated
                || quiet_uninstall_field.truncated
                || windows_installer.truncated
                || system_component.truncated
                || no_remove.truncated
                || parent_key_name.truncated
                || release_type.truncated
                || display_icon_path_truncated;
            for (field, state, expected) in [
                (
                    "DisplayVersion",
                    version_field.state,
                    "REG_SZ/REG_EXPAND_SZ",
                ),
                ("Publisher", publisher_field.state, "REG_SZ/REG_EXPAND_SZ"),
                (
                    "InstallLocation",
                    install_location_field.state,
                    "REG_SZ/REG_EXPAND_SZ",
                ),
                (
                    "InstallDate",
                    install_date_field.state,
                    "REG_SZ/REG_EXPAND_SZ",
                ),
                (
                    "DisplayIcon",
                    display_icon_field.state,
                    "REG_SZ/REG_EXPAND_SZ",
                ),
                (
                    "UninstallString",
                    uninstall_field.state,
                    "REG_SZ/REG_EXPAND_SZ",
                ),
                (
                    "QuietUninstallString",
                    quiet_uninstall_field.state,
                    "REG_SZ/REG_EXPAND_SZ",
                ),
                ("WindowsInstaller", windows_installer.state, "REG_DWORD"),
                ("SystemComponent", system_component.state, "REG_DWORD"),
                ("NoRemove", no_remove.state, "REG_DWORD"),
                (
                    "ParentKeyName",
                    parent_key_name.state,
                    "REG_SZ/REG_EXPAND_SZ",
                ),
                ("ReleaseType", release_type.state, "REG_SZ/REG_EXPAND_SZ"),
            ] {
                if is_unreadable(state) {
                    item_truncated = true;
                    push_item_field_issue(
                        &mut item_issues,
                        &bounded_key.value,
                        field,
                        expected,
                        state,
                    );
                }
            }
            if install_date_state == RegistryValueState::Invalid {
                item_truncated = true;
                item_issues.push(item_issue(
                    &bounded_key.value,
                    "SOFTWARE-INSTALL-DATE-INVALID",
                    "InstallDate 存在但无法规范化为 YYYY-MM-DD；保留为无效，不猜测日期。",
                ));
            }
            if display_icon_field
                .value
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                && display_icon_path.is_none()
            {
                item_issues.push(item_issue(
                    &bounded_key.value,
                    "SOFTWARE-DISPLAY-ICON-UNRESOLVED",
                    "DisplayIcon 存在但无法安全解析为静态路径；不展示原始值。",
                ));
            }
            if display_icon_path_truncated {
                item_issues.push(item_issue(
                    &bounded_key.value,
                    "SOFTWARE-DISPLAY-ICON-TRUNCATED",
                    "DisplayIcon 路径超过安全长度上限，已截断。",
                ));
            }
            if item_truncated {
                item_issues.push(item_issue(
                    &bounded_key.value,
                    "SOFTWARE-FIELD-TRUNCATED",
                    "至少一个软件字段被截断或无法完整读取；条目保留为部分未知。",
                ));
            }

            let observation = if item_truncated {
                InventoryObservation::Partial
            } else {
                InventoryObservation::Observed
            };
            if observation != InventoryObservation::Observed {
                partial_items += 1;
                source_partial = true;
            }

            let risk = assess_software(SoftwareFacts {
                uninstall_command_present: uninstall_command_present
                    || quiet_uninstall_command_present,
                uninstall_command_known: !is_unreadable(uninstall_field.state)
                    && !is_unreadable(quiet_uninstall_field.state),
                install_location_state,
                classification,
            });

            let id = software_entry_id(location.scope, location.view, &hive_and_key);
            if !seen_ids.insert(id.clone()) {
                duplicate_entries += 1;
                continue;
            }

            items.push(InstalledSoftware {
                id,
                display_name,
                version: non_empty(version_field.value),
                version_state: version_field.state,
                publisher: non_empty(publisher_field.value),
                publisher_state: publisher_field.state,
                install_location,
                install_location_state,
                install_location_value_state: install_location_field.state,
                install_date,
                install_date_state,
                display_icon_path,
                display_icon_state: display_icon_field.state,
                windows_installer: windows_installer.value,
                windows_installer_state: windows_installer.state,
                uninstall_command_present,
                quiet_uninstall_command_present,
                scope: location.scope,
                registry_view: location.view,
                registry_key: bounded_key.value,
                system_component: system_component.value,
                system_component_state: system_component.state,
                no_remove: no_remove.value,
                no_remove_state: no_remove.state,
                parent_key_name: parent_key_name.value,
                parent_key_name_state: parent_key_name.state,
                release_type: release_type.value,
                release_type_state: release_type.state,
                classification,
                observation,
                issues: item_issues,
                risk,
            });
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

    if truncated_strings > 0 {
        truncated = true;
        issues.push(DataIssue {
            code: "registryStringTruncated".to_string(),
            message: format!(
                "{truncated_strings} 个注册表文本值超过 {MAX_REGISTRY_STRING_CHARS} 字符上限，已截断并以“…”标记。"
            ),
            source: "Uninstall registry values".to_string(),
        });
    }

    if access_denied_paths > 0 {
        issues.push(DataIssue {
            code: "installLocationAccessDenied".to_string(),
            message: format!(
                "{access_denied_paths} 个普通软件安装路径访问被拒绝，状态保持未知而不是“不存在”。"
            ),
            source: "Uninstall registry InstallLocation".to_string(),
        });
    }

    if duplicate_entries > 0 {
        issues.push(DataIssue {
            code: "duplicateRegistryEntry".to_string(),
            message: format!("{duplicate_entries} 个重复注册表条目已被抑制，不重复展示。"),
            source: "Uninstall registry".to_string(),
        });
    }
    if entries_without_display_name > 0 {
        issues.push(DataIssue {
            code: "entriesWithoutDisplayName".to_string(),
            message: format!(
                "{entries_without_display_name} 个卸载注册项没有可用的 DisplayName；已跳过且不计为软件条目。"
            ),
            source: "Uninstall registry".to_string(),
        });
    }
    if display_name_unreadable > 0 {
        issues.push(DataIssue {
            code: "displayNameUnreadable".to_string(),
            message: format!(
                "{display_name_unreadable} 个卸载注册项的 DisplayName 类型异常或读取失败；未把它们当作不存在的软件。"
            ),
            source: "Uninstall registry".to_string(),
        });
    }
    if partial_items > 0 {
        issues.push(DataIssue {
            code: "softwarePartialItems".to_string(),
            message: format!("{partial_items} 个软件条目保留为部分未知；字段问题逐项记录。"),
            source: "Uninstall registry".to_string(),
        });
    }
    let observation = aggregate_observation(&sources, partial_items, truncated);
    if items.is_empty() && observation == InventoryObservation::Observed {
        issues.push(DataIssue {
            code: "softwareItemsEmpty".to_string(),
            message: "已覆盖的 Uninstall 位置没有返回可显示软件条目；这不代表系统没有安装软件。"
                .to_string(),
            source: "Uninstall registry".to_string(),
        });
    }
    if items.is_empty() && observation != InventoryObservation::Observed {
        issues.push(DataIssue {
            code: "softwareSourcesUnavailable".to_string(),
            message: "软件来源未能完整读取；空列表不代表没有安装软件。".to_string(),
            source: "Uninstall registry".to_string(),
        });
    }

    items.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });

    SoftwareScanResult {
        checked_at,
        observation,
        sources,
        items,
        issues,
        limitations: vec![
            "仅读取传统 Uninstall 注册表项；不包含 Microsoft Store/AppX、MSIX 或便携软件。"
                .to_string(),
            "HKCU 只读取共享用户视图一次；HKLM 分别读取 32/64 位视图，避免 HKCU 重复。".to_string(),
            "未关联服务、计划任务、启动项、文件签名、浏览器扩展或卸载残留。".to_string(),
            "系统组件/组件类条目不等同普通用户软件；无法确认驱动或运行时类别时保持 Unknown。"
                .to_string(),
            format!(
                "单个注册表视图最多检查 {MAX_SOFTWARE_ITEMS_PER_VIEW} 项，所有视图合计最多保留 {MAX_SOFTWARE_ITEMS_TOTAL} 项；达到上限会标记结果不完整。"
            ),
            format!(
                "单个注册表文本值最多 {MAX_REGISTRY_STRING_CHARS} 字符；DisplayIcon 只保留解析后的静态路径，最多 {MAX_SOFTWARE_ICON_PATH_CHARS} 字符；超限会截断并标记。"
            ),
            "安装路径的 Missing、AccessDenied 和 Unknown 是不同状态；访问被拒绝不会被当成不存在。"
                .to_string(),
            "网络共享、设备路径和相对路径不会进行存在性访问，结果保持未知。".to_string(),
            "UninstallString/QuietUninstallString 只记录是否存在非空值；不返回、不保存完整卸载命令，也不执行卸载程序。"
                .to_string(),
            "未验证软件名称、发布者或安装路径的安全性或恶意性；这些字段不能单独作为结论。"
                .to_string(),
        ],
        truncated,
    }
}

fn normalized_install_date(
    field: &ParsedRegistryValue<String>,
) -> (Option<String>, RegistryValueState) {
    let Some(raw) = field.value.as_deref() else {
        return (None, field.state);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return (None, field.state);
    }
    if field.state != RegistryValueState::Present {
        return (None, field.state);
    }

    let value = normalize_install_date(Some(trimmed));
    let state = if has_date_shape(trimmed) {
        RegistryValueState::Present
    } else {
        RegistryValueState::Invalid
    };
    if state == RegistryValueState::Invalid {
        (None, state)
    } else {
        (value, state)
    }
}

fn has_date_shape(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 4
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || *byte == b'-' || *byte == b'/')
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn read_string_field(
    key: &RegKey,
    name: &str,
    truncated_strings: &mut usize,
) -> ParsedRegistryValue<String> {
    let parsed = match key.get_raw_value(name) {
        Ok(raw) => parse_string(
            Some((registry_value_kind(raw.vtype), raw.bytes.as_ref())),
            MAX_REGISTRY_STRING_CHARS,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            parse_string(None, MAX_REGISTRY_STRING_CHARS)
        }
        Err(_) => ParsedRegistryValue::read_error(),
    };
    if parsed.truncated {
        *truncated_strings += 1;
    }
    parsed
}

fn read_dword_field(key: &RegKey, name: &str) -> ParsedRegistryValue<u32> {
    match key.get_raw_value(name) {
        Ok(raw) => parse_dword(Some((registry_value_kind(raw.vtype), raw.bytes.as_ref()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => parse_dword(None),
        Err(_) => ParsedRegistryValue::read_error(),
    }
}

/// Extracts a static icon path clue. DisplayIcon commonly has a trailing `,index`
/// and may be quoted; the raw value is never returned.
fn parse_display_icon_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_index = if let Some(rest) = trimmed.strip_prefix('"') {
        let closing = rest.find('"')?;
        let path = &rest[..closing];
        if path.trim().is_empty() {
            return None;
        }
        path
    } else if let Some((path, index)) = trimmed.rsplit_once(',') {
        if !index.trim().is_empty()
            && index.trim().bytes().all(|byte| byte.is_ascii_digit())
            && !path.trim().is_empty()
        {
            path.trim()
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    Some(without_index.trim().to_string()).filter(|value| !value.is_empty())
}

fn push_item_field_issue(
    issues: &mut Vec<DataIssue>,
    source: &str,
    field: &str,
    expected: &str,
    state: RegistryValueState,
) {
    let code = match state {
        RegistryValueState::TypeMismatch => "SOFTWARE-FIELD-TYPE-MISMATCH",
        RegistryValueState::Invalid => "SOFTWARE-FIELD-INVALID",
        RegistryValueState::ReadError => "SOFTWARE-FIELD-READ-ERROR",
        RegistryValueState::Present | RegistryValueState::Missing => return,
    };
    issues.push(item_issue(
        source,
        code,
        &format!("字段 {field} 期望 {expected}；读取状态为 {state:?}，未按缺失处理。"),
    ));
}

fn item_issue(source: &str, code: &str, message: &str) -> DataIssue {
    DataIssue {
        code: code.to_string(),
        message: message.to_string(),
        source: source.to_string(),
    }
}

fn deadline_reached(started: Instant) -> bool {
    started.elapsed() >= SOFTWARE_SCAN_TIME_LIMIT
}

fn mark_time_limited(truncated: &mut bool, time_limited: &mut bool, issues: &mut Vec<DataIssue>) {
    *truncated = true;
    if !*time_limited {
        *time_limited = true;
        issues.push(DataIssue {
            code: "softwareTimeLimit".to_string(),
            message: "软件盘点超过 8 秒安全上限，已停止继续读取；结果可能不完整。".to_string(),
            source: "Uninstall registry".to_string(),
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

fn aggregate_observation(
    sources: &[SoftwareSourceStatus],
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

fn registry_value_kind(value_type: RegType) -> RegistryValueKind {
    match value_type {
        REG_SZ => RegistryValueKind::String,
        REG_EXPAND_SZ => RegistryValueKind::ExpandString,
        REG_DWORD => RegistryValueKind::Dword,
        _ => RegistryValueKind::Other,
    }
}

fn read_classification_dword(
    key: &RegKey,
    field_name: &str,
    source: &str,
    issues: &mut Vec<DataIssue>,
) -> ParsedRegistryValue<u32> {
    let parsed = match key.get_raw_value(field_name) {
        Ok(raw) => parse_dword(Some((registry_value_kind(raw.vtype), raw.bytes.as_ref()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => parse_dword(None),
        Err(error) => {
            push_registry_field_issue(
                issues,
                source,
                field_name,
                "REG_DWORD",
                RegistryValueState::ReadError,
                Some(&error),
            );
            return ParsedRegistryValue::read_error();
        }
    };

    push_registry_field_issue(issues, source, field_name, "REG_DWORD", parsed.state, None);
    parsed
}

fn read_classification_string(
    key: &RegKey,
    field_name: &str,
    source: &str,
    truncated_strings: &mut usize,
    issues: &mut Vec<DataIssue>,
) -> ParsedRegistryValue<String> {
    let parsed = match key.get_raw_value(field_name) {
        Ok(raw) => parse_string(
            Some((registry_value_kind(raw.vtype), raw.bytes.as_ref())),
            MAX_REGISTRY_STRING_CHARS,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            parse_string(None, MAX_REGISTRY_STRING_CHARS)
        }
        Err(error) => {
            push_registry_field_issue(
                issues,
                source,
                field_name,
                "REG_SZ/REG_EXPAND_SZ",
                RegistryValueState::ReadError,
                Some(&error),
            );
            return ParsedRegistryValue::read_error();
        }
    };

    if parsed.truncated {
        *truncated_strings += 1;
    }
    push_registry_field_issue(
        issues,
        source,
        field_name,
        "REG_SZ/REG_EXPAND_SZ",
        parsed.state,
        None,
    );
    parsed
}

fn push_registry_field_issue(
    issues: &mut Vec<DataIssue>,
    source: &str,
    field_name: &str,
    expected: &str,
    state: RegistryValueState,
    error: Option<&std::io::Error>,
) {
    let (code, description) = match state {
        RegistryValueState::TypeMismatch => (
            "registryValueTypeMismatch",
            "字段类型与预期不一致，未按缺失处理。",
        ),
        RegistryValueState::Invalid => ("registryValueInvalid", "字段存在但值无效，未按缺失处理。"),
        RegistryValueState::ReadError => ("registryValueReadError", "字段读取失败，未按缺失处理。"),
        RegistryValueState::Present | RegistryValueState::Missing => return,
    };

    let suffix = error
        .map(|error| format!(" 原因：{error}"))
        .unwrap_or_default();
    issues.push(DataIssue {
        code: code.to_string(),
        message: format!("字段 {field_name} 期望 {expected}；{description}{suffix}"),
        source: source.to_string(),
    });
}

fn software_entry_id(scope: InstallScope, view: RegistryView, hive_and_key: &str) -> String {
    let token = stable_token(hive_and_key);
    match scope {
        InstallScope::User => format!("inventory:user:{token}"),
        InstallScope::Machine => {
            format!("inventory:machine:{}:{token}", registry_view_id(view))
        }
    }
}

fn registry_view_id(view: RegistryView) -> &'static str {
    match view {
        RegistryView::Registry32 => "registry32",
        RegistryView::Registry64 => "registry64",
        RegistryView::Shared => "shared",
    }
}

fn source_id(location: &UninstallLocation) -> String {
    format!(
        "{}:{}:{}",
        location.hive_name,
        registry_view_id(location.view),
        UNINSTALL_KEY
    )
}

fn registry_issue(source: &str, error: &std::io::Error, message: &str) -> DataIssue {
    DataIssue {
        code: if is_access_denied(error) {
            "registryAccessDenied".to_string()
        } else {
            "registryReadFailed".to_string()
        },
        message: format!("{message} 原因：{error}"),
        source: source.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hkcu_is_scanned_once_without_wow64_duplication() {
        let user_locations: Vec<_> = LOCATIONS
            .iter()
            .filter(|location| location.scope == InstallScope::User)
            .collect();

        assert_eq!(user_locations.len(), 1);
        assert_eq!(user_locations[0].view, RegistryView::Shared);
        assert_eq!(user_locations[0].view_flag, 0);
    }

    #[test]
    fn machine_registry_views_remain_distinct() {
        let machine_locations: Vec<_> = LOCATIONS
            .iter()
            .filter(|location| location.scope == InstallScope::Machine)
            .collect();

        assert_eq!(machine_locations.len(), 2);
        assert_ne!(
            machine_locations[0].view_flag,
            machine_locations[1].view_flag
        );
    }

    #[test]
    fn hkcu_identity_does_not_include_a_registry_view_selector() {
        let key = r"HKEY_CURRENT_USER\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\App";
        let first = software_entry_id(InstallScope::User, RegistryView::Shared, key);
        let second = software_entry_id(InstallScope::User, RegistryView::Shared, key);

        assert_eq!(first, second);
        assert!(!first.contains("registry32"));
        assert!(!first.contains("registry64"));
    }

    #[test]
    fn display_icon_paths_are_static_clues_not_commands() {
        assert_eq!(
            parse_display_icon_path(r#""C:\Program Files\App\app.exe",0"#),
            Some(r"C:\Program Files\App\app.exe".to_string())
        );
        assert_eq!(
            parse_display_icon_path(r"C:\Program Files\App\app.exe,0"),
            Some(r"C:\Program Files\App\app.exe".to_string())
        );
        assert_eq!(
            parse_display_icon_path(r"C:\App\app.ico"),
            Some(r"C:\App\app.ico".to_string())
        );
        assert_eq!(parse_display_icon_path("   "), None);
    }

    #[test]
    fn empty_fields_are_missing_values_not_defaults() {
        assert_eq!(non_empty(Some("  ".to_string())), None);
        assert_eq!(non_empty(None), None);
        assert_eq!(
            non_empty(Some(" value ".to_string())),
            Some("value".to_string())
        );
    }

    #[test]
    fn invalid_install_date_is_not_fabricated() {
        let field = ParsedRegistryValue {
            value: Some("not-a-date".to_string()),
            state: RegistryValueState::Present,
            truncated: false,
        };
        let (value, state) = normalized_install_date(&field);
        assert_eq!(value, None);
        assert_eq!(state, RegistryValueState::Invalid);
    }

    #[test]
    fn empty_install_date_is_empty_not_invalid_or_fabricated() {
        let field = ParsedRegistryValue {
            value: Some("  ".to_string()),
            state: RegistryValueState::Present,
            truncated: false,
        };
        let (value, state) = normalized_install_date(&field);
        assert_eq!(value, None);
        assert_eq!(state, RegistryValueState::Present);
    }

    #[test]
    fn date_shapes_keep_partial_but_numeric_dates() {
        assert!(has_date_shape("2024"));
        assert!(has_date_shape("2024-01"));
        assert!(has_date_shape("2024/01/31"));
        assert!(!has_date_shape("unknown"));
        assert!(!has_date_shape(""));
    }

    #[test]
    fn serialized_software_contract_contains_only_uninstall_presence() {
        let item = InstalledSoftware {
            id: "inventory:machine:registry64:test".to_string(),
            display_name: "Example".to_string(),
            version: None,
            version_state: RegistryValueState::Missing,
            publisher: None,
            publisher_state: RegistryValueState::Missing,
            install_location: None,
            install_location_state: PathState::Unknown,
            install_location_value_state: RegistryValueState::Missing,
            install_date: None,
            install_date_state: RegistryValueState::Missing,
            display_icon_path: None,
            display_icon_state: RegistryValueState::Missing,
            windows_installer: None,
            windows_installer_state: RegistryValueState::Missing,
            uninstall_command_present: true,
            quiet_uninstall_command_present: false,
            scope: InstallScope::Machine,
            registry_view: RegistryView::Registry64,
            registry_key: "HKEY_LOCAL_MACHINE\\...\\Uninstall\\Example".to_string(),
            system_component: None,
            system_component_state: RegistryValueState::Missing,
            no_remove: None,
            no_remove_state: RegistryValueState::Missing,
            parent_key_name: None,
            parent_key_name_state: RegistryValueState::Missing,
            release_type: None,
            release_type_state: RegistryValueState::Missing,
            classification: SoftwareClassification::UserSoftware,
            observation: InventoryObservation::Observed,
            issues: Vec::new(),
            risk: assess_software(SoftwareFacts {
                uninstall_command_present: true,
                uninstall_command_known: true,
                install_location_state: PathState::Unknown,
                classification: SoftwareClassification::UserSoftware,
            }),
        };

        let value = serde_json::to_value(&item).expect("serialize software item");
        assert!(value.get("uninstallCommand").is_none());
        assert!(value.get("quietUninstallCommand").is_none());
        assert_eq!(value["uninstallCommandPresent"], true);
        assert_eq!(value["quietUninstallCommandPresent"], false);
        assert!(!value.to_string().contains("uninstall.exe"));
    }

    #[test]
    fn source_aggregation_does_not_treat_absent_as_observed_data() {
        let observed = SoftwareSourceStatus {
            id: "observed".to_string(),
            scope: InstallScope::Machine,
            registry_view: RegistryView::Registry64,
            registry_key: "key".to_string(),
            observation: InventoryObservation::Observed,
            exists: Some(true),
            item_count: 1,
        };
        let absent = SoftwareSourceStatus {
            id: "absent".to_string(),
            scope: InstallScope::Machine,
            registry_view: RegistryView::Registry32,
            registry_key: "key".to_string(),
            observation: InventoryObservation::Unavailable,
            exists: Some(false),
            item_count: 0,
        };
        let denied = SoftwareSourceStatus {
            id: "denied".to_string(),
            scope: InstallScope::User,
            registry_view: RegistryView::Shared,
            registry_key: "key".to_string(),
            observation: InventoryObservation::AccessDenied,
            exists: None,
            item_count: 0,
        };

        assert_eq!(
            aggregate_observation(&[observed.clone(), absent], 0, false),
            InventoryObservation::Observed
        );
        assert_eq!(
            aggregate_observation(&[observed, denied], 0, false),
            InventoryObservation::Partial
        );

        let denied_only = SoftwareSourceStatus {
            id: "denied-only".to_string(),
            scope: InstallScope::User,
            registry_view: RegistryView::Shared,
            registry_key: "key".to_string(),
            observation: InventoryObservation::AccessDenied,
            exists: None,
            item_count: 0,
        };
        assert_eq!(
            aggregate_observation(&[denied_only], 0, false),
            InventoryObservation::AccessDenied
        );

        let error_only = SoftwareSourceStatus {
            id: "error-only".to_string(),
            scope: InstallScope::Machine,
            registry_view: RegistryView::Registry64,
            registry_key: "key".to_string(),
            observation: InventoryObservation::Error,
            exists: None,
            item_count: 0,
        };
        assert_eq!(
            aggregate_observation(&[error_only], 0, false),
            InventoryObservation::Error
        );
    }
}
