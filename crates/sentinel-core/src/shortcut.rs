//! Read-only Windows shortcut (`.lnk`) metadata resolution.
//!
//! Only `IPersistFile::Load` and getters are used. The resolver never invokes
//! `IShellLinkW::Resolve`, never executes the target, and never follows a
//! network or device path. Argument text is inspected only to record whether it
//! was non-empty; the text is not returned or logged.

use std::path::Path;

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::core::{Interface, PCWSTR};

use crate::com::ComApartment;
use crate::limits::{
    MAX_STARTUP_PATH_CHARS, MAX_STARTUP_SHORTCUT_ARGUMENT_CHARS,
    MAX_STARTUP_SHORTCUT_WORKDIR_CHARS, bound_text,
};
use crate::model::InventoryObservation;
use crate::win32::{string_from_wide_until_nul, wide_null};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShortcutIssue {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShortcutResolution {
    pub target_path: Option<String>,
    pub target_truncated: bool,
    pub arguments_present: Option<bool>,
    pub working_directory_present: Option<bool>,
    pub observation: InventoryObservation,
    pub issues: Vec<ShortcutIssue>,
}

pub(crate) struct ShortcutResolver {
    apartment: Option<ComApartment>,
    init_error: Option<u32>,
}

impl ShortcutResolver {
    pub(crate) fn new() -> Self {
        Self {
            apartment: None,
            init_error: None,
        }
    }

    pub(crate) fn resolve(&mut self, path: &Path) -> ShortcutResolution {
        if self.apartment.is_none() && self.init_error.is_none() {
            match ComApartment::initialize() {
                Ok(apartment) => self.apartment = Some(apartment),
                Err(code) => self.init_error = Some(code),
            }
        }
        if let Some(code) = self.init_error {
            return ShortcutResolution {
                target_path: None,
                target_truncated: false,
                arguments_present: None,
                working_directory_present: None,
                observation: InventoryObservation::Error,
                issues: vec![ShortcutIssue {
                    code: "STARTUP-SHORTCUT-COM-INIT-FAILED",
                    message: format!(
                        "无法初始化 COM 以解析快捷方式（HRESULT 0x{code:08X}）；目标保持未知。"
                    ),
                }],
            };
        }

        match resolve_loaded(path) {
            Ok(resolution) => resolution,
            Err(error) => {
                let code = error.code().0 as u32;
                let observation = if code & 0xFFFF == 5 {
                    InventoryObservation::AccessDenied
                } else {
                    InventoryObservation::Error
                };
                ShortcutResolution {
                    target_path: None,
                    target_truncated: false,
                    arguments_present: None,
                    working_directory_present: None,
                    observation,
                    issues: vec![ShortcutIssue {
                        code: "STARTUP-SHORTCUT-READ-FAILED",
                        message: format!(
                            "无法读取快捷方式元数据（HRESULT 0x{code:08X}）；条目保留，目标保持未知。"
                        ),
                    }],
                }
            }
        }
    }
}

fn resolve_loaded(path: &Path) -> windows::core::Result<ShortcutResolution> {
    let path_wide = wide_null(&path.to_string_lossy());
    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
    let persist: IPersistFile = shell_link.cast()?;
    unsafe { persist.Load(PCWSTR(path_wide.as_ptr()), STGM_READ)? };

    let mut issues = Vec::new();
    // Read more than the result cap so a >1,024-character target is detected as
    // truncated instead of being silently reported as a complete 1,024-char path.
    let mut path_buffer = vec![0u16; MAX_STARTUP_PATH_CHARS * 4 + 1];
    unsafe { shell_link.GetPath(&mut path_buffer, std::ptr::null_mut(), 0)? };
    let buffer_truncated = !path_buffer.contains(&0);
    let raw_target = string_from_wide_until_nul(&path_buffer);
    let bounded_target = bound_text(&raw_target, MAX_STARTUP_PATH_CHARS);
    let path_truncated = buffer_truncated || bounded_target.truncated;
    let target_path = if bounded_target.value.trim().is_empty() {
        issues.push(ShortcutIssue {
            code: "STARTUP-SHORTCUT-TARGET-UNRESOLVED",
            message: "快捷方式没有返回目标路径；不推测目标文件。".to_string(),
        });
        None
    } else {
        Some(bounded_target.value)
    };
    if path_truncated {
        issues.push(ShortcutIssue {
            code: "STARTUP-SHORTCUT-PATH-TRUNCATED",
            message: format!(
                "快捷方式目标路径超过 {MAX_STARTUP_PATH_CHARS} 字符上限，已截断显示。"
            ),
        });
    }

    let arguments_present = read_presence(
        &mut issues,
        || unsafe {
            let mut buffer = vec![0u16; MAX_STARTUP_SHORTCUT_ARGUMENT_CHARS + 1];
            shell_link.GetArguments(&mut buffer).map(|()| buffer)
        },
        "STARTUP-SHORTCUT-ARGUMENTS-UNREADABLE",
        "参数",
    );

    let working_directory_present = read_presence(
        &mut issues,
        || unsafe {
            let mut buffer = vec![0u16; MAX_STARTUP_SHORTCUT_WORKDIR_CHARS + 1];
            shell_link.GetWorkingDirectory(&mut buffer).map(|()| buffer)
        },
        "STARTUP-SHORTCUT-WORKDIR-UNREADABLE",
        "工作目录",
    );

    let observation = if target_path.is_some()
        && !path_truncated
        && arguments_present.is_some()
        && working_directory_present.is_some()
    {
        InventoryObservation::Observed
    } else {
        InventoryObservation::Partial
    };

    Ok(ShortcutResolution {
        target_path,
        target_truncated: path_truncated,
        arguments_present,
        working_directory_present,
        observation,
        issues,
    })
}

fn read_presence<F>(
    issues: &mut Vec<ShortcutIssue>,
    read: F,
    error_code: &'static str,
    label: &'static str,
) -> Option<bool>
where
    F: FnOnce() -> windows::core::Result<Vec<u16>>,
{
    match read() {
        Ok(buffer) => Some(!string_from_wide_until_nul(&buffer).trim().is_empty()),
        Err(error) => {
            issues.push(ShortcutIssue {
                code: error_code,
                message: format!(
                    "快捷方式{label}读取失败（HRESULT 0x{:08X}）；不显示其内容。",
                    error.code().0 as u32
                ),
            });
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_shortcut_path() -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "sentinel-lite-startup-test-{}-{stamp}.lnk",
            std::process::id()
        ))
    }

    /// Test-only fixture writer. It creates a unique `.lnk` under the OS temp
    /// directory and never touches a real Startup folder, registry Run key, or
    /// any existing shortcut. The caller removes the file after resolution.
    fn create_test_shortcut(path: &Path) {
        let _apartment = ComApartment::initialize().expect("COM initialization");
        let shell_link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
                .expect("shell link");
        let persist: IPersistFile = shell_link.cast().expect("IPersistFile");
        let target = wide_null(r"C:\Windows\System32\notepad.exe");
        let arguments = wide_null("--sentinel-test");
        let workdir = wide_null(r"C:\Windows");
        let file = wide_null(&path.to_string_lossy());
        unsafe {
            shell_link
                .SetPath(PCWSTR(target.as_ptr()))
                .expect("set path");
            shell_link
                .SetArguments(PCWSTR(arguments.as_ptr()))
                .expect("set arguments");
            shell_link
                .SetWorkingDirectory(PCWSTR(workdir.as_ptr()))
                .expect("set workdir");
            persist
                .Save(PCWSTR(file.as_ptr()), true)
                .expect("save shortcut");
        }
    }

    #[test]
    fn resolves_shortcut_target_and_only_reports_presence_flags() {
        let path = unique_shortcut_path();
        create_test_shortcut(&path);

        let mut resolver = ShortcutResolver::new();
        let resolution = resolver.resolve(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(resolution.observation, InventoryObservation::Observed);
        assert!(
            resolution
                .target_path
                .as_deref()
                .is_some_and(|value| value.to_ascii_lowercase().ends_with("notepad.exe"))
        );
        assert_eq!(resolution.arguments_present, Some(true));
        assert_eq!(resolution.working_directory_present, Some(true));
        assert!(resolution.issues.is_empty());
    }

    #[test]
    fn missing_shortcut_is_retained_as_an_error_without_panicking() {
        let path = unique_shortcut_path();
        let mut resolver = ShortcutResolver::new();
        let resolution = resolver.resolve(&path);

        assert!(matches!(
            resolution.observation,
            InventoryObservation::Error
                | InventoryObservation::AccessDenied
                | InventoryObservation::Unavailable
        ));
        assert!(resolution.target_path.is_none());
        assert!(!resolution.issues.is_empty());
    }
}
