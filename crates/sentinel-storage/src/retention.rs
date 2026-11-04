//! Bounded retention for the local telemetry database.

use serde::{Deserialize, Serialize};

pub const DEFAULT_MAX_AGE_DAYS: u32 = 30;
pub const DEFAULT_MAX_EVENTS: u64 = 1_000_000;
pub const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_SESSION_ROWS: u64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub max_age_days: u32,
    pub max_events: u64,
    pub max_bytes: u64,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_age_days: DEFAULT_MAX_AGE_DAYS,
            max_events: DEFAULT_MAX_EVENTS,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }
}

impl RetentionPolicy {
    pub fn validate(&self) -> Result<(), String> {
        if self.max_age_days == 0 {
            return Err("retention max_age_days must be greater than zero".to_owned());
        }
        if self.max_age_days > 3650 {
            return Err("retention max_age_days must be at most 3650".to_owned());
        }
        if self.max_events < 1_000 {
            return Err("retention max_events must be at least 1000".to_owned());
        }
        if self.max_bytes < 16 * 1024 * 1024 {
            return Err("retention max_bytes must be at least 16 MiB".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct RetentionStats {
    pub deleted_events: u64,
    pub deleted_invalid: u64,
    pub deleted_revisions: u64,
    pub deleted_sessions: u64,
    pub database_bytes: u64,
}
