use std::collections::HashMap;

/// Hard cap on entries examined in one registry view.
pub const MAX_SOFTWARE_ITEMS_PER_VIEW: usize = 512;
/// Hard cap on normalized software entries across every view.
pub const MAX_SOFTWARE_ITEMS_TOTAL: usize = 1024;
/// Hard cap on startup values examined in one registry view.
pub const MAX_STARTUP_ITEMS_PER_VIEW: usize = 256;
/// Hard cap on startup entries across registry and startup folders.
pub const MAX_STARTUP_ITEMS_TOTAL: usize = 512;
/// Hard cap on a registry string before it crosses IPC or enters the UI.
pub const MAX_REGISTRY_STRING_CHARS: usize = 2_048;
/// Hard cap on the parsed DisplayIcon path returned to the UI.
pub const MAX_SOFTWARE_ICON_PATH_CHARS: usize = 1_024;
/// Hard cap on startup value/file display names.
pub const MAX_STARTUP_NAME_CHARS: usize = 512;
/// Hard cap on a startup target executable path returned to the UI.
pub const MAX_STARTUP_PATH_CHARS: usize = 1_024;
/// Read-only buffer for shortcut arguments; the text itself is never retained.
pub const MAX_STARTUP_SHORTCUT_ARGUMENT_CHARS: usize = 2_048;
/// Read-only buffer for a shortcut working directory; the text itself is never retained.
pub const MAX_STARTUP_SHORTCUT_WORKDIR_CHARS: usize = 1_024;
/// Bound on optional on-demand SHA-256 evidence for a selected process file.
pub const MAX_FILE_HASH_BYTES: u64 = 256 * 1024 * 1024;
/// Hard cap on process entries collected from a Toolhelp32 snapshot.
pub const MAX_PROCESS_ITEMS: usize = 2_048;
/// Hard cap on process/parent display names.
pub const MAX_PROCESS_NAME_CHARS: usize = 512;
/// Hard cap on process executable path text crossing IPC/UI.
pub const MAX_PROCESS_PATH_CHARS: usize = 4_096;
/// Hard cap on service entries returned by one SCM snapshot.
pub const MAX_SERVICE_ITEMS: usize = 1_024;
/// Hard cap on service names/display names.
pub const MAX_SERVICE_NAME_CHARS: usize = 512;
/// Hard cap on the service account string.
pub const MAX_SERVICE_ACCOUNT_CHARS: usize = 512;
/// Hard cap on the service executable path (arguments are never retained).
pub const MAX_SERVICE_PATH_CHARS: usize = 1_024;
/// Hard cap on scheduled task entries returned by one snapshot.
pub const MAX_TASK_ITEMS: usize = 1_024;
/// Hard cap on task folders visited in one snapshot.
pub const MAX_TASK_FOLDERS: usize = 256;
/// Hard cap on task names.
pub const MAX_TASK_NAME_CHARS: usize = 512;
/// Hard cap on the full task path.
pub const MAX_TASK_PATH_CHARS: usize = 1_024;
/// Hard cap on task author/run-as identity text.
pub const MAX_TASK_IDENTITY_CHARS: usize = 512;
/// Hard cap on a task action executable path (arguments are never retained).
pub const MAX_TASK_ACTION_PATH_CHARS: usize = 1_024;
/// Hard cap on actions returned for one task.
pub const MAX_TASK_ACTIONS: usize = 16;
/// Hard cap on association relations returned to the UI.
pub const MAX_ASSOCIATION_RELATIONS: usize = 2_048;
/// Hard cap on filename-only weak associations.
pub const MAX_ASSOCIATION_WEAK_RELATIONS: usize = 256;
/// Hard cap on relations for one software asset before the result is marked partial.
pub const MAX_ASSOCIATION_RELATIONS_PER_SOFTWARE: usize = 256;
/// Hard cap on distinct software candidates retained for one target.
pub const MAX_ASSOCIATION_CANDIDATES_PER_TARGET: usize = 32;
/// Hard cap on redacted association evidence text.
pub const MAX_ASSOCIATION_EVIDENCE_CHARS: usize = 1_024;
/// Hard cap on association issues before the response is marked partial.
pub const MAX_ASSOCIATION_ISSUES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedText {
    pub value: String,
    pub truncated: bool,
}

/// Bounds a string by Unicode scalar values, preserving UTF-8 boundaries.
pub fn bound_text(raw: &str, max_chars: usize) -> BoundedText {
    let char_count = raw.chars().count();
    if char_count <= max_chars {
        return BoundedText {
            value: raw.to_string(),
            truncated: false,
        };
    }

    if max_chars == 0 {
        return BoundedText {
            value: String::new(),
            truncated: true,
        };
    }

    let mut value: String = raw.chars().take(max_chars - 1).collect();
    value.push('…');
    BoundedText {
        value,
        truncated: true,
    }
}

/// Enforces per-source and total limits while a scanner is enumerating external input.
#[derive(Debug)]
pub struct SourceBudget {
    per_source_limit: usize,
    total_limit: usize,
    processed_by_source: HashMap<String, usize>,
    total_items: usize,
}

impl SourceBudget {
    pub fn new(per_source_limit: usize, total_limit: usize) -> Self {
        Self {
            per_source_limit,
            total_limit,
            processed_by_source: HashMap::new(),
            total_items: 0,
        }
    }

    pub fn can_process(&self, source: &str) -> bool {
        self.total_items < self.total_limit
            && self.processed_by_source.get(source).copied().unwrap_or(0) < self.per_source_limit
    }

    pub fn record_processed(&mut self, source: &str) {
        *self
            .processed_by_source
            .entry(source.to_string())
            .or_insert(0) += 1;
    }

    pub fn record_item(&mut self) {
        self.total_items += 1;
    }

    pub fn total_items(&self) -> usize {
        self.total_items
    }
}
