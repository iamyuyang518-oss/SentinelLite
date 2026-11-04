use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use sha2::Digest;

use crate::model::PathState;

pub const FILETIME_EPOCH_DIFFERENCE: u64 = 116_444_736_000_000_000;

const PROCESS_IDENTITY_PREFIX: &str = "pidct1:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessIdentity {
    creation_ticks: u64,
}

impl ProcessIdentity {
    pub fn from_ticks(creation_ticks: u64) -> Option<Self> {
        if creation_ticks == 0 {
            None
        } else {
            Some(Self { creation_ticks })
        }
    }

    pub fn as_ticks(self) -> u64 {
        self.creation_ticks
    }

    /// Encodes the high-precision FILETIME creation timestamp as an opaque identity token.
    pub fn token(self) -> String {
        format!("{PROCESS_IDENTITY_PREFIX}{:016x}", self.creation_ticks)
    }

    pub fn parse(token: &str) -> Option<Self> {
        let hex = token.strip_prefix(PROCESS_IDENTITY_PREFIX)?;
        if hex.len() != 16 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let ticks = u64::from_str_radix(hex, 16).ok()?;
        Self::from_ticks(ticks)
    }

    pub fn matches(self, creation_ticks: u64) -> bool {
        self.creation_ticks == creation_ticks
    }
}

/// Stable short hash for identifiers. It prevents unbounded registry names from crossing IPC.
pub fn stable_token(value: &str) -> String {
    use std::fmt::Write as _;

    let digest = sha2::Sha256::digest(value.as_bytes());
    let mut output = String::with_capacity(32);
    for byte in digest.iter().take(16) {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

pub fn path_state_from_io_error(error: &io::Error) -> PathState {
    if error.raw_os_error() == Some(5) || error.kind() == io::ErrorKind::PermissionDenied {
        PathState::AccessDenied
    } else if error.kind() == io::ErrorKind::NotFound {
        PathState::Missing
    } else {
        PathState::Unknown
    }
}

pub fn system_time_to_rfc3339(time: SystemTime) -> Option<String> {
    let timestamp = DateTime::<Utc>::from(time);
    Some(timestamp.to_rfc3339_opts(SecondsFormat::Secs, true))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedCommandPath {
    Quoted(String),
    Unquoted(String),
    AmbiguousUnquoted { candidate: Option<String> },
    Unresolved,
}

/// Parses a startup command without probing or executing it.
///
/// Windows accepts unquoted executable paths that contain spaces, but the exact executable can
/// be ambiguous. Such commands are surfaced as `AmbiguousUnquoted` instead of being treated as
/// a missing file.
pub fn parse_startup_command(command: &str) -> ParsedCommandPath {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return ParsedCommandPath::Unresolved;
    }

    if let Some(rest) = trimmed.strip_prefix('"') {
        return match rest.find('"') {
            Some(closing) => {
                let path = rest.get(..closing).unwrap_or_default().trim();
                if path.is_empty() {
                    ParsedCommandPath::Unresolved
                } else {
                    ParsedCommandPath::Quoted(path.to_string())
                }
            }
            None => ParsedCommandPath::Unresolved,
        };
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.is_empty() {
        return ParsedCommandPath::Unresolved;
    }
    if tokens.len() == 1 || has_executable_extension(tokens[0]) {
        return ParsedCommandPath::Unquoted(tokens[0].to_string());
    }

    let first_looks_like_path = looks_like_path_start(tokens[0]);
    let later_executable = tokens
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, token)| has_executable_extension(token))
        .map(|(index, _)| index);

    if first_looks_like_path {
        let candidate = later_executable.map(|index| tokens[..=index].join(" "));
        return ParsedCommandPath::AmbiguousUnquoted { candidate };
    }

    ParsedCommandPath::Unquoted(tokens[0].to_string())
}

fn has_executable_extension(token: &str) -> bool {
    const EXECUTABLE_EXTENSIONS: [&str; 10] = [
        ".exe", ".com", ".bat", ".cmd", ".ps1", ".vbs", ".js", ".jse", ".wsf", ".msi",
    ];
    let lower = token.to_ascii_lowercase();
    EXECUTABLE_EXTENSIONS
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn looks_like_path_start(token: &str) -> bool {
    token.contains('\\')
        || token.contains('/')
        || token.starts_with('%')
        || token.starts_with("\\\\")
}

/// Converts a Windows FILETIME tick count into a second-precision RFC 3339 UTC string.
pub fn filetime_ticks_to_rfc3339(ticks: u64) -> Option<String> {
    if ticks == 0 || ticks < FILETIME_EPOCH_DIFFERENCE {
        return None;
    }

    let seconds_since_1970 = ticks
        .checked_sub(FILETIME_EPOCH_DIFFERENCE)?
        .checked_div(10_000_000)?;
    let nanos = ((ticks % 10_000_000) * 100) as u32;
    let timestamp = DateTime::<Utc>::from_timestamp(seconds_since_1970 as i64, nanos)?;
    Some(timestamp.to_rfc3339_opts(SecondsFormat::Secs, true))
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Returns a bitset's logical CPU indices without shifting past the 64-bit boundary.
pub fn mask_to_cpu_indices(mask: u64) -> Vec<u32> {
    (0u32..64).filter(|bit| mask & (1u64 << bit) != 0).collect()
}

pub fn mask_to_hex(mask: u64) -> String {
    format!("{mask:#018x}")
}

/// A restricted affinity mask is a non-zero strict subset of the system mask.
pub fn is_affinity_restricted(process_mask: u64, system_mask: u64) -> bool {
    process_mask != 0
        && system_mask != 0
        && process_mask != system_mask
        && process_mask & system_mask == process_mask
}

/// Percentage of the total CPU capacity (all logical processors) used during a window.
pub fn calculate_cpu_percent(
    process_cpu_delta: Duration,
    wall_delta: Duration,
    logical_processors: u32,
) -> Option<f64> {
    if wall_delta.is_zero() || logical_processors == 0 {
        return None;
    }

    let capacity_seconds = wall_delta.as_secs_f64() * f64::from(logical_processors);
    if capacity_seconds <= 0.0 {
        return None;
    }

    Some(((process_cpu_delta.as_secs_f64() / capacity_seconds) * 100.0).clamp(0.0, 100.0))
}

pub fn normalize_install_date(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.len() == 8 && trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y%m%d") {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }

    Some(trimmed.to_string())
}

/// Only drive-absolute local paths are eligible for metadata/signature probes.
///
/// UNC and device namespaces are deliberately excluded so a read-only scan does not contact a
/// remote share or device. Relative commands remain unresolved instead of being treated as
/// missing.
pub fn is_local_absolute_path(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed.starts_with(r"\\") || trimmed.starts_with("//") {
        return false;
    }

    Path::new(trimmed).is_absolute()
}

pub fn is_temp_path(path: &str) -> bool {
    let normalized = path.replace('/', "\\").to_ascii_lowercase();
    normalized.contains("\\appdata\\local\\temp\\")
        || normalized.contains("\\windows\\temp\\")
        || normalized.ends_with("\\temp")
}
