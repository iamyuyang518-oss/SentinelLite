//! Presentation-boundary redaction helpers.
//!
//! The scanners keep raw paths for matching and local checks. The Tauri command
//! layer applies these helpers before returning inventory data to the WebView.

pub fn redact_user_profile_path(raw: &str) -> String {
    let replaced = raw.trim().replace('/', "\\");
    let (prefix, rest) = if let Some(rest) = replaced.strip_prefix(r"\\?\") {
        (r"\\?\".to_string(), rest)
    } else if let Some(rest) = replaced.strip_prefix(r"\??\") {
        (r"\??\".to_string(), rest)
    } else {
        (String::new(), replaced.as_str())
    };
    let lower = rest.to_ascii_lowercase();
    if lower.len() > "c:\\users\\".len()
        && lower.as_bytes()[0].is_ascii_alphabetic()
        && lower.as_bytes()[1] == b':'
        && lower.as_bytes()[2] == b'\\'
        && lower["c:\\".len()..].starts_with("users\\")
    {
        let user_start = 3 + "users\\".len();
        let user_end = rest[user_start..]
            .find('\\')
            .map(|offset| user_start + offset)
            .unwrap_or(rest.len());
        if user_end > user_start {
            return format!(
                "{prefix}{}\\***{}",
                &rest[..user_start.saturating_sub(1)],
                &rest[user_end..]
            );
        }
    }
    replaced
}

/// Redacts profile-user segments even when the path is embedded in a sentence.
pub fn redact_user_profile_paths_in_text(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    let marker = "\\users\\";
    let mut output = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    let mut search_from = 0usize;

    while let Some(relative) = lower[search_from..].find(marker) {
        let marker_start = search_from + relative;
        let user_start = marker_start + marker.len();
        let user_end = raw[user_start..]
            .char_indices()
            .find(|(_, character)| matches!(*character, '\\' | '"' | '\'' | '“' | '”'))
            .map(|(offset, _)| user_start + offset)
            .unwrap_or(raw.len());
        if user_end == user_start {
            search_from = user_start;
            continue;
        }
        let has_drive_prefix = marker_start >= 2
            && lower.as_bytes()[marker_start - 2].is_ascii_alphabetic()
            && lower.as_bytes()[marker_start - 1] == b':';
        if !has_drive_prefix {
            search_from = user_end;
            continue;
        }
        output.push_str(&raw[cursor..user_start]);
        output.push_str("***");
        cursor = user_end;
        search_from = user_end;
    }

    output.push_str(&raw[cursor..]);
    output
}

/// Preserves well-known system and group identities, but replaces personal
/// account portions with `***`. This is presentation-only; scanners keep the
/// original account text for their own checks.
/// Replaces a known current-user name inside display-only text. The caller
/// must only pass a non-empty account name; matching is case-insensitive.
pub fn redact_username_in_text(raw: &str, username: &str) -> String {
    if username.len() < 3 {
        return raw.to_string();
    }
    let lower = raw.to_ascii_lowercase();
    let needle = username.to_ascii_lowercase();
    let mut output = String::with_capacity(raw.len());
    let mut cursor = 0usize;
    let mut search_from = 0usize;
    while let Some(relative) = lower[search_from..].find(&needle) {
        let start = search_from + relative;
        output.push_str(&raw[cursor..start]);
        output.push_str("***");
        cursor = start + needle.len();
        search_from = cursor;
    }
    output.push_str(&raw[cursor..]);
    output
}

pub fn redact_account_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let upper = trimmed.to_ascii_uppercase();
    const KNOWN_IDENTITIES: &[&str] = &[
        "SYSTEM",
        "LOCAL SYSTEM",
        "LOCALSYSTEM",
        "LOCAL SERVICE",
        "LOCALSERVICE",
        "NETWORK SERVICE",
        "NETWORKSERVICE",
        "TRUSTEDINSTALLER",
        "EVERYONE",
        "USERS",
        "ADMINISTRATORS",
        "AUTHENTICATED USERS",
        "INTERACTIVE",
        "SERVICE",
        "NETWORK",
        "BATCH",
        "LOCAL",
    ];
    if KNOWN_IDENTITIES.contains(&upper.as_str()) || upper.starts_with("S-1-") {
        return trimmed.to_string();
    }
    if let Some((domain, user)) = trimmed.rsplit_once('\\') {
        let domain_upper = domain.to_ascii_uppercase();
        const SYSTEM_DOMAINS: &[&str] = &[
            "NT AUTHORITY",
            "NT SERVICE",
            "BUILTIN",
            "WINDOW MANAGER",
            "FONT DRIVER HOST",
        ];
        if SYSTEM_DOMAINS.contains(&domain_upper.as_str()) && !user.is_empty() {
            return trimmed.to_string();
        }
        if user.is_empty() {
            return trimmed.to_string();
        }
        return format!("{domain}\\***");
    }
    "***".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_profile_user_while_preserving_path_suffix() {
        assert_eq!(
            redact_user_profile_path(r"C:\Users\Alice\AppData\Local\App\app.exe"),
            r"C:\Users\***\AppData\Local\App\app.exe"
        );
        assert_eq!(
            redact_user_profile_path(r"\\?\C:\Users\Alice\AppData\app.exe"),
            r"\\?\C:\Users\***\AppData\app.exe"
        );
        assert_eq!(
            redact_user_profile_path(r"C:\Program Files\App\app.exe"),
            r"C:\Program Files\App\app.exe"
        );
    }

    #[test]
    fn redacts_profile_paths_inside_risk_evidence_text() {
        assert_eq!(
            redact_user_profile_paths_in_text(
                r"目标路径“C:\Users\Alice\AppData\Local\Temp\app.exe”位于临时目录。"
            ),
            r"目标路径“C:\Users\***\AppData\Local\Temp\app.exe”位于临时目录。"
        );
        assert_eq!(
            redact_user_profile_paths_in_text(r"没有用户目录的 C:\Program Files\App\app.exe"),
            r"没有用户目录的 C:\Program Files\App\app.exe"
        );
        assert_eq!(
            redact_user_profile_paths_in_text(r"C:\Users\Alice"),
            r"C:\Users\***"
        );
    }

    #[test]
    fn redacts_known_username_inside_task_display_text() {
        assert_eq!(
            redact_username_in_text("WpsUpdateTask_Alice", "Alice"),
            "WpsUpdateTask_***"
        );
        assert_eq!(
            redact_username_in_text("WpsUpdateTask_alice", "Alice"),
            "WpsUpdateTask_***"
        );
        assert_eq!(redact_username_in_text("Task", "Alice"), "Task");
        assert_eq!(redact_username_in_text("Task", "a"), "Task");
    }

    #[test]
    fn preserves_system_accounts_and_redacts_personal_accounts() {
        for value in [
            "NT AUTHORITY\\SYSTEM",
            "NT AUTHORITY\\LocalService",
            "NT SERVICE\\TrustedInstaller",
            "BUILTIN\\Administrators",
            "S-1-5-18",
        ] {
            assert_eq!(redact_account_name(value), value);
        }
        assert_eq!(redact_account_name("CONTOSO\\alice"), "CONTOSO\\***");
        assert_eq!(redact_account_name(".\\alice"), ".\\***");
        assert_eq!(redact_account_name("alice"), "***");
    }
}
