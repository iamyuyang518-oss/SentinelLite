//! Minimal sensor health accounting.
//!
//! The contract is fail-closed: a run is only healthy when it stopped cleanly,
//! observed and processed events, had balanced counters, and had a known zero
//! loss count. Nothing here can produce a "protected" claim.

use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use serde::Serialize;

use crate::time::{filetime_to_unix_ms, unix_ms_to_rfc3339};

const MAX_ERROR_CHARS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum SensorState {
    NotStarted = 0,
    Starting = 1,
    Active = 2,
    Degraded = 3,
    Failed = 4,
    Stopped = 5,
    Unavailable = 6,
}

impl SensorState {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Starting,
            2 => Self::Active,
            3 => Self::Degraded,
            4 => Self::Failed,
            5 => Self::Stopped,
            6 => Self::Unavailable,
            _ => Self::NotStarted,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum PermissionState {
    Unknown = 0,
    Granted = 1,
    Limited = 2,
    Denied = 3,
}

impl PermissionState {
    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Granted,
            2 => Self::Limited,
            3 => Self::Denied,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthSnapshot {
    pub state: SensorState,
    pub permission_state: PermissionState,
    pub events_received: u64,
    pub events_processed: u64,
    pub events_dropped: u64,
    pub events_lost: Option<u64>,
    pub realtime_buffers_lost: Option<u64>,
    pub decode_errors: u64,
    pub partial_events: u64,
    pub ignored_events: u64,
    pub last_event_at: Option<String>,
    pub last_error: Option<String>,
    pub degraded: bool,
    pub ok: bool,
}

#[derive(Debug)]
pub struct SensorHealth {
    state: AtomicU8,
    permission: AtomicU8,
    events_received: AtomicU64,
    events_processed: AtomicU64,
    events_dropped: AtomicU64,
    decode_errors: AtomicU64,
    partial_events: AtomicU64,
    ignored_events: AtomicU64,
    last_event_at_ms: AtomicU64,
    events_lost: Mutex<Option<u64>>,
    realtime_buffers_lost: Mutex<Option<u64>>,
    last_error: Mutex<Option<String>>,
}

impl Default for SensorHealth {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorHealth {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(SensorState::NotStarted as u8),
            permission: AtomicU8::new(PermissionState::Unknown as u8),
            events_received: AtomicU64::new(0),
            events_processed: AtomicU64::new(0),
            events_dropped: AtomicU64::new(0),
            decode_errors: AtomicU64::new(0),
            partial_events: AtomicU64::new(0),
            ignored_events: AtomicU64::new(0),
            last_event_at_ms: AtomicU64::new(0),
            events_lost: Mutex::new(None),
            realtime_buffers_lost: Mutex::new(None),
            last_error: Mutex::new(None),
        }
    }

    pub fn state(&self) -> SensorState {
        SensorState::from_u8(self.state.load(Ordering::Relaxed))
    }

    pub fn permission_state(&self) -> PermissionState {
        PermissionState::from_u8(self.permission.load(Ordering::Relaxed))
    }

    pub fn set_permission(&self, permission: PermissionState) {
        self.permission.store(permission as u8, Ordering::Relaxed);
    }

    fn transition(&self, target: SensorState, allowed: &[SensorState]) -> bool {
        let target = target as u8;
        loop {
            let current = self.state.load(Ordering::Relaxed);
            if !allowed.contains(&SensorState::from_u8(current)) {
                return false;
            }
            if self
                .state
                .compare_exchange(current, target, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return true;
            }
        }
    }

    pub fn mark_starting(&self) {
        self.transition(
            SensorState::Starting,
            &[SensorState::NotStarted, SensorState::Stopped],
        );
    }

    pub fn mark_active(&self) {
        self.transition(
            SensorState::Active,
            &[SensorState::NotStarted, SensorState::Starting],
        );
    }

    pub fn mark_stopped(&self) {
        self.transition(
            SensorState::Stopped,
            &[
                SensorState::Starting,
                SensorState::Active,
                SensorState::Stopped,
            ],
        );
    }

    pub fn mark_degraded(&self, reason: &str) {
        if self.transition(
            SensorState::Degraded,
            &[
                SensorState::NotStarted,
                SensorState::Starting,
                SensorState::Active,
                SensorState::Degraded,
                SensorState::Stopped,
            ],
        ) {
            self.set_last_error(reason);
        }
    }

    pub fn mark_failed(&self, reason: &str) {
        self.transition(
            SensorState::Failed,
            &[
                SensorState::NotStarted,
                SensorState::Starting,
                SensorState::Active,
                SensorState::Degraded,
                SensorState::Stopped,
                SensorState::Unavailable,
            ],
        );
        self.set_last_error(reason);
    }

    pub fn mark_unavailable(&self, reason: &str) {
        self.transition(
            SensorState::Unavailable,
            &[
                SensorState::NotStarted,
                SensorState::Starting,
                SensorState::Stopped,
            ],
        );
        self.set_last_error(reason);
    }

    pub fn record_received(&self) {
        self.events_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_processed(&self) {
        self.events_processed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_ignored(&self) {
        self.ignored_events.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_dropped(&self, reason: &str) {
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
        self.mark_degraded(reason);
    }

    /// Record a bounded-queue drop without taking the last-error lock from the
    /// ETW callback. The pipeline reports the reason at shutdown/status time.
    pub fn record_queue_dropped(&self) {
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_decode_error(&self, reason: &str) {
        self.events_dropped.fetch_add(1, Ordering::Relaxed);
        self.decode_errors.fetch_add(1, Ordering::Relaxed);
        self.mark_degraded(reason);
    }

    pub fn record_partial(&self, reason: &str) {
        self.partial_events.fetch_add(1, Ordering::Relaxed);
        self.mark_degraded(reason);
    }

    pub fn record_integrity_issue(&self, reason: &str) {
        self.mark_degraded(reason);
    }

    pub fn record_event_time(&self, filetime: u64) {
        if let Some(ms) = filetime_to_unix_ms(filetime) {
            self.last_event_at_ms.fetch_max(ms, Ordering::Relaxed);
        }
    }

    pub fn set_events_lost(&self, value: Option<u64>) {
        merge_optional_count(&self.events_lost, value);
    }

    pub fn set_realtime_buffers_lost(&self, value: Option<u64>) {
        merge_optional_count(&self.realtime_buffers_lost, value);
    }

    pub fn set_last_error(&self, reason: &str) {
        let message = sanitize_message(reason);
        if message.is_empty() {
            return;
        }
        let mut guard = self
            .last_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(message);
    }

    pub fn snapshot(&self) -> HealthSnapshot {
        let state = self.state();
        let events_received = self.events_received.load(Ordering::Relaxed);
        let events_processed = self.events_processed.load(Ordering::Relaxed);
        let events_dropped = self.events_dropped.load(Ordering::Relaxed);
        let decode_errors = self.decode_errors.load(Ordering::Relaxed);
        let partial_events = self.partial_events.load(Ordering::Relaxed);
        let ignored_events = self.ignored_events.load(Ordering::Relaxed);
        let events_lost = *self
            .events_lost
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let realtime_buffers_lost = *self
            .realtime_buffers_lost
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let last_error = self
            .last_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let last_event_at = match self.last_event_at_ms.load(Ordering::Relaxed) {
            0 => None,
            ms => unix_ms_to_rfc3339(ms),
        };

        let accounting_balanced = events_received
            == events_processed
                .saturating_add(events_dropped)
                .saturating_add(ignored_events);
        let ok = state == SensorState::Stopped
            && events_received > 0
            && events_processed > 0
            && events_dropped == 0
            && decode_errors == 0
            && partial_events == 0
            && events_lost == Some(0)
            && realtime_buffers_lost == Some(0)
            && accounting_balanced;

        HealthSnapshot {
            state,
            permission_state: self.permission_state(),
            events_received,
            events_processed,
            events_dropped,
            events_lost,
            realtime_buffers_lost,
            decode_errors,
            partial_events,
            ignored_events,
            last_event_at,
            last_error,
            degraded: state == SensorState::Degraded,
            ok,
        }
    }
}

fn merge_optional_count(slot: &Mutex<Option<u64>>, value: Option<u64>) {
    let Some(value) = value else {
        return;
    };
    let mut guard = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = Some(match *guard {
        Some(current) => current.max(value),
        None => value,
    });
}

fn sanitize_message(message: &str) -> String {
    let mut output = String::new();
    for (count, character) in message.chars().enumerate() {
        if count == MAX_ERROR_CHARS {
            break;
        }
        output.push(if character.is_control() {
            ' '
        } else {
            character
        });
    }
    output.trim().to_owned()
}

impl fmt::Display for SensorState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotStarted => "not_started",
            Self::Starting => "starting",
            Self::Active => "active",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Unavailable => "unavailable",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::FILETIME_UNIX_EPOCH_DIFF_100NS;

    fn clean_health() -> SensorHealth {
        let health = SensorHealth::new();
        health.mark_starting();
        health.mark_active();
        health.record_received();
        health.record_processed();
        health.set_events_lost(Some(0));
        health.set_realtime_buffers_lost(Some(0));
        health.mark_stopped();
        health
    }

    #[test]
    fn initial_state_is_not_started() {
        let snapshot = SensorHealth::new().snapshot();
        assert_eq!(snapshot.state, SensorState::NotStarted);
        assert_eq!(snapshot.permission_state, PermissionState::Unknown);
        assert!(!snapshot.ok);
    }

    #[test]
    fn starting_then_active() {
        let health = SensorHealth::new();
        health.mark_starting();
        assert_eq!(health.state(), SensorState::Starting);
        health.mark_active();
        assert_eq!(health.state(), SensorState::Active);
    }

    #[test]
    fn decode_error_degrades_and_counts() {
        let health = SensorHealth::new();
        health.mark_starting();
        health.mark_active();
        health.record_received();
        health.record_decode_error("bad payload");
        health.mark_stopped();

        let snapshot = health.snapshot();
        assert_eq!(snapshot.state, SensorState::Degraded);
        assert_eq!(snapshot.events_dropped, 1);
        assert_eq!(snapshot.decode_errors, 1);
        assert_eq!(snapshot.events_received, 1);
        assert!(!snapshot.ok);
    }

    #[test]
    fn failed_is_not_overwritten_by_stop() {
        let health = SensorHealth::new();
        health.mark_starting();
        health.mark_failed("boom");
        health.mark_stopped();
        assert_eq!(health.state(), SensorState::Failed);
    }

    #[test]
    fn degraded_is_not_overwritten_by_stop() {
        let health = SensorHealth::new();
        health.mark_starting();
        health.mark_active();
        health.record_dropped("lossy queue");
        health.mark_stopped();
        assert_eq!(health.state(), SensorState::Degraded);
    }

    #[test]
    fn degraded_does_not_overwrite_a_fatal_error() {
        let health = SensorHealth::new();
        health.mark_starting();
        health.set_permission(PermissionState::Denied);
        health.mark_unavailable("StartTraceW failed: 5: access denied");
        health.mark_degraded("no process start events observed");

        let snapshot = health.snapshot();
        assert_eq!(snapshot.state, SensorState::Unavailable);
        assert_eq!(
            snapshot.last_error.as_deref(),
            Some("StartTraceW failed: 5: access denied")
        );
    }

    #[test]
    fn permission_denied_is_unavailable() {
        let health = SensorHealth::new();
        health.set_permission(PermissionState::Denied);
        health.mark_starting();
        health.mark_unavailable("access denied");
        let snapshot = health.snapshot();
        assert_eq!(snapshot.state, SensorState::Unavailable);
        assert_eq!(snapshot.permission_state, PermissionState::Denied);
        assert!(!snapshot.ok);
    }

    #[test]
    fn clean_run_can_be_ok() {
        let snapshot = clean_health().snapshot();
        assert_eq!(snapshot.state, SensorState::Stopped);
        assert_eq!(snapshot.events_received, 1);
        assert_eq!(snapshot.events_processed, 1);
        assert_eq!(snapshot.events_lost, Some(0));
        assert_eq!(snapshot.realtime_buffers_lost, Some(0));
        assert!(snapshot.ok);
    }

    #[test]
    fn zero_events_is_not_ok() {
        let health = SensorHealth::new();
        health.mark_starting();
        health.mark_active();
        health.set_events_lost(Some(0));
        health.set_realtime_buffers_lost(Some(0));
        health.mark_stopped();
        assert!(!health.snapshot().ok);
    }

    #[test]
    fn unknown_loss_count_is_not_ok() {
        let health = SensorHealth::new();
        health.mark_starting();
        health.mark_active();
        health.record_received();
        health.record_processed();
        health.set_realtime_buffers_lost(Some(0));
        health.mark_stopped();

        let snapshot = health.snapshot();
        assert_eq!(snapshot.events_lost, None);
        assert!(!snapshot.ok);
    }

    #[test]
    fn unbalanced_counters_are_not_ok() {
        let health = SensorHealth::new();
        health.mark_starting();
        health.mark_active();
        health.record_received();
        health.record_received();
        health.record_processed();
        health.set_events_lost(Some(0));
        health.set_realtime_buffers_lost(Some(0));
        health.mark_stopped();
        assert!(!health.snapshot().ok);
    }

    #[test]
    fn event_loss_degrades_the_session() {
        let health = SensorHealth::new();
        health.mark_starting();
        health.mark_active();
        health.set_events_lost(Some(2));
        health.set_realtime_buffers_lost(Some(0));
        health.mark_degraded("events lost");
        health.mark_stopped();

        let snapshot = health.snapshot();
        assert_eq!(snapshot.state, SensorState::Degraded);
        assert_eq!(snapshot.events_lost, Some(2));
        assert!(!snapshot.ok);
    }

    #[test]
    fn last_event_time_is_derived_from_filetime() {
        let expected =
            chrono::DateTime::parse_from_rfc3339("2026-10-31T00:00:00Z").expect("test timestamp");
        let unix_seconds = expected.timestamp() as u64;
        let filetime = (unix_seconds + FILETIME_UNIX_EPOCH_DIFF_100NS / 10_000_000) * 10_000_000;

        let health = SensorHealth::new();
        health.record_event_time(filetime);
        let snapshot = health.snapshot();
        assert_eq!(
            snapshot.last_event_at.as_deref(),
            Some("2026-10-31T00:00:00.000Z")
        );
    }

    #[test]
    fn zero_filetime_has_no_event_time() {
        let health = SensorHealth::new();
        health.record_event_time(0);
        assert_eq!(health.snapshot().last_event_at, None);
    }

    #[test]
    fn last_error_is_bounded_and_sanitized() {
        let health = SensorHealth::new();
        health.mark_degraded(&format!("bad\n\u{7}{}", "x".repeat(2000)));
        let message = health.snapshot().last_error.expect("error message");
        assert!(message.chars().count() <= MAX_ERROR_CHARS);
        assert!(!message.contains('\n'));
        assert!(!message.contains('\u{7}'));
    }
}
