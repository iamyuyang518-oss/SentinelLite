//! Session-local process start/stop pairing.
//!
//! `(pid, create_time)` is the persistent identity. The provider's process
//! sequence number is used only as a cross-check when both sides carry one.
//! A stop never pairs across a different `(pid, create_time)`.
//!
//! The state machine never guesses. A stop that arrives before its start is
//! retained for a bounded retroactive pairing window; if a matching start
//! arrives later the pair is marked out-of-order and the caller emits a
//! revision. Duplicate stops are detected against a bounded memory of closed
//! identities. A zero `create_time` is not a usable identity and is reported
//! as indeterminate instead of being paired by PID alone.

use std::collections::{HashMap, VecDeque};

use serde::Serialize;

use crate::decode::DecodedProcessEvent;

pub use sentinel_events::PairingSnapshot;

pub const MAX_OBSERVED_START_IMAGES: usize = 64;
pub const MAX_OBSERVED_IMAGE_NAME_CHARS: usize = 512;
pub const MAX_OPEN_STARTS: usize = 65_536;
pub const MAX_CLOSED_IDENTITIES: usize = 65_536;
pub const MAX_PENDING_ORPHAN_STOPS: usize = 4_096;
pub const MAX_OPEN_START_SAMPLES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ProcessKey {
    pub pid: u32,
    pub create_time: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingOutcome {
    StartOpened,
    StartDuplicate {
        first_event_sequence: u64,
    },
    StartPairedEarlierStop {
        stop_event_sequence: u64,
        sequence_match: Option<bool>,
    },
    StartOverflow,
    StopPaired {
        start_event_sequence: u64,
        sequence_match: Option<bool>,
    },
    StopOrphan {
        retained: bool,
    },
    StopDuplicate {
        first_event_sequence: u64,
    },
    StopIdentityMismatch {
        open_create_time: u64,
        retained: bool,
    },
    IdentityIndeterminate,
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenStartSample {
    pub event_sequence: u64,
    pub pid: u32,
    pub create_time: u64,
}

#[derive(Debug, Clone)]
struct OpenStart {
    event_sequence: u64,
    etw_sequence: Option<u64>,
}

#[derive(Debug, Clone)]
struct ClosedIdentity {
    start_event_sequence: u64,
    stop_event_sequence: u64,
}

#[derive(Debug, Clone)]
struct OrphanStop {
    event_sequence: u64,
    etw_sequence: Option<u64>,
}

#[derive(Debug, Default)]
pub struct PairingState {
    starts: HashMap<ProcessKey, OpenStart>,
    open_by_pid: HashMap<u32, ProcessKey>,
    closed: HashMap<ProcessKey, ClosedIdentity>,
    closed_order: VecDeque<ProcessKey>,
    orphan_stops: HashMap<ProcessKey, OrphanStop>,
    orphan_order: VecDeque<ProcessKey>,
    starts_seen: u64,
    stops_seen: u64,
    paired: u64,
    orphan_stops_seen: u64,
    duplicate_starts: u64,
    duplicate_stops: u64,
    pid_reuse_detected: u64,
    sequence_mismatches: u64,
    start_overflow: u64,
    orphan_stop_overflow: u64,
    identity_mismatched_stops: u64,
    out_of_order_pairs: u64,
    indeterminate_identities: u64,
    closed_identity_evictions: u64,
    observed_start_image_names: Vec<String>,
    observed_start_image_names_truncated: bool,
}

impl PairingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe(&mut self, event: &DecodedProcessEvent, event_sequence: u64) -> PairingOutcome {
        if event.create_time == 0 {
            self.indeterminate_identities += 1;
            return PairingOutcome::IdentityIndeterminate;
        }
        if event.is_start() {
            self.starts_seen += 1;
            self.observe_start(event, event_sequence)
        } else if event.is_stop() {
            self.stops_seen += 1;
            self.observe_stop(event, event_sequence)
        } else {
            PairingOutcome::Ignored
        }
    }

    fn observe_start(
        &mut self,
        event: &DecodedProcessEvent,
        event_sequence: u64,
    ) -> PairingOutcome {
        let key = ProcessKey {
            pid: event.pid,
            create_time: event.create_time,
        };
        if let Some(open) = self.starts.get(&key) {
            self.duplicate_starts += 1;
            return PairingOutcome::StartDuplicate {
                first_event_sequence: open.event_sequence,
            };
        }
        if let Some(closed) = self.closed.get(&key) {
            self.duplicate_starts += 1;
            return PairingOutcome::StartDuplicate {
                first_event_sequence: closed.start_event_sequence,
            };
        }
        if let Some(orphan) = self.take_orphan_stop(&key) {
            let sequence_match = sequence_match(event.sequence, orphan.etw_sequence);
            if sequence_match == Some(false) {
                self.sequence_mismatches += 1;
            }
            self.remember_closed(key, event_sequence, orphan.event_sequence);
            self.out_of_order_pairs += 1;
            self.record_observed_image(event.image_name.as_deref());
            return PairingOutcome::StartPairedEarlierStop {
                stop_event_sequence: orphan.event_sequence,
                sequence_match,
            };
        }
        if self.starts.len() >= MAX_OPEN_STARTS {
            self.start_overflow += 1;
            self.record_observed_image(event.image_name.as_deref());
            return PairingOutcome::StartOverflow;
        }
        if let Some(previous) = self.open_by_pid.get(&key.pid) {
            if previous.create_time != key.create_time {
                self.pid_reuse_detected += 1;
            }
        }

        self.open_by_pid.insert(key.pid, key);
        self.starts.insert(
            key,
            OpenStart {
                event_sequence,
                etw_sequence: event.sequence,
            },
        );
        self.record_observed_image(event.image_name.as_deref());
        PairingOutcome::StartOpened
    }

    fn observe_stop(&mut self, event: &DecodedProcessEvent, event_sequence: u64) -> PairingOutcome {
        let key = ProcessKey {
            pid: event.pid,
            create_time: event.create_time,
        };
        if let Some(start) = self.starts.remove(&key) {
            if self.open_by_pid.get(&key.pid) == Some(&key) {
                self.open_by_pid.remove(&key.pid);
            }
            self.paired += 1;
            let sequence_match = sequence_match(start.etw_sequence, event.sequence);
            if sequence_match == Some(false) {
                self.sequence_mismatches += 1;
            }
            self.remember_closed(key, start.event_sequence, event_sequence);
            return PairingOutcome::StopPaired {
                start_event_sequence: start.event_sequence,
                sequence_match,
            };
        }
        if let Some(closed) = self.closed.get(&key) {
            self.duplicate_stops += 1;
            return PairingOutcome::StopDuplicate {
                first_event_sequence: closed.stop_event_sequence,
            };
        }
        if let Some(open) = self.open_by_pid.get(&key.pid) {
            let open_create_time = open.create_time;
            if open_create_time != key.create_time {
                self.identity_mismatched_stops += 1;
                let retained = self.remember_orphan_stop(key, event_sequence, event);
                return PairingOutcome::StopIdentityMismatch {
                    open_create_time,
                    retained,
                };
            }
        }
        if let Some(orphan) = self.orphan_stops.get(&key) {
            self.duplicate_stops += 1;
            return PairingOutcome::StopDuplicate {
                first_event_sequence: orphan.event_sequence,
            };
        }
        let retained = self.remember_orphan_stop(key, event_sequence, event);
        PairingOutcome::StopOrphan { retained }
    }

    fn remember_orphan_stop(
        &mut self,
        key: ProcessKey,
        event_sequence: u64,
        event: &DecodedProcessEvent,
    ) -> bool {
        if self.orphan_stops.len() >= MAX_PENDING_ORPHAN_STOPS {
            self.orphan_stop_overflow += 1;
            self.orphan_stops_seen += 1;
            return false;
        }
        self.orphan_stops_seen += 1;
        self.orphan_stops.insert(
            key,
            OrphanStop {
                event_sequence,
                etw_sequence: event.sequence,
            },
        );
        self.orphan_order.push_back(key);
        true
    }

    fn take_orphan_stop(&mut self, key: &ProcessKey) -> Option<OrphanStop> {
        let orphan = self.orphan_stops.remove(key)?;
        self.orphan_order.retain(|queued| queued != key);
        Some(orphan)
    }

    fn remember_closed(
        &mut self,
        key: ProcessKey,
        start_event_sequence: u64,
        stop_event_sequence: u64,
    ) {
        if !self.closed.contains_key(&key) {
            if self.closed.len() >= MAX_CLOSED_IDENTITIES {
                if let Some(oldest) = self.closed_order.pop_front() {
                    if self.closed.remove(&oldest).is_some() {
                        self.closed_identity_evictions += 1;
                    }
                }
            }
            self.closed_order.push_back(key);
        }
        self.closed.insert(
            key,
            ClosedIdentity {
                start_event_sequence,
                stop_event_sequence,
            },
        );
    }

    fn record_observed_image(&mut self, image: Option<&str>) {
        let Some(image) = image else {
            return;
        };
        let name = image_basename(image);
        if name.is_empty() {
            return;
        }
        let (name, name_truncated) = truncate_chars(name, MAX_OBSERVED_IMAGE_NAME_CHARS);
        if name_truncated {
            self.observed_start_image_names_truncated = true;
        }
        if self
            .observed_start_image_names
            .iter()
            .any(|existing| existing == name)
        {
            return;
        }
        if self.observed_start_image_names.len() < MAX_OBSERVED_START_IMAGES {
            self.observed_start_image_names.push(name.to_owned());
        } else {
            self.observed_start_image_names_truncated = true;
        }
    }

    pub fn open_start_samples(&self, limit: usize) -> Vec<OpenStartSample> {
        self.starts
            .iter()
            .take(limit.min(MAX_OPEN_START_SAMPLES))
            .map(|(key, start)| OpenStartSample {
                event_sequence: start.event_sequence,
                pid: key.pid,
                create_time: key.create_time,
            })
            .collect()
    }

    pub fn snapshot(&self) -> PairingSnapshot {
        PairingSnapshot {
            paired: self.paired,
            unpaired_starts: self.starts.len() as u64,
            orphan_stops: self.orphan_stops_seen,
            duplicate_starts: self.duplicate_starts,
            duplicate_stops: self.duplicate_stops,
            pid_reuse_detected: self.pid_reuse_detected,
            sequence_mismatches: self.sequence_mismatches,
            start_overflow: self.start_overflow,
            orphan_stop_overflow: self.orphan_stop_overflow,
            identity_mismatched_stops: self.identity_mismatched_stops,
            out_of_order_pairs: self.out_of_order_pairs,
            indeterminate_identities: self.indeterminate_identities,
            closed_identity_evictions: self.closed_identity_evictions,
            pending_orphan_stops: self.orphan_stops.len() as u64,
            starts_seen: self.starts_seen,
            stops_seen: self.stops_seen,
            observed_start_image_names: self.observed_start_image_names.clone(),
            observed_start_image_names_truncated: self.observed_start_image_names_truncated,
        }
    }
}

fn sequence_match(start: Option<u64>, stop: Option<u64>) -> Option<bool> {
    match (start, stop) {
        (Some(start), Some(stop)) => Some(start == stop),
        _ => None,
    }
}

fn image_basename(image: &str) -> &str {
    image.rsplit(['\\', '/']).next().unwrap_or(image)
}

fn truncate_chars(value: &str, max_chars: usize) -> (&str, bool) {
    match value.char_indices().nth(max_chars) {
        Some((index, _)) => (&value[..index], true),
        None => (value, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{EVENT_PROCESS_START, EVENT_PROCESS_STOP, FieldState};

    fn event(
        event_id: u16,
        pid: u32,
        create_time: u64,
        sequence: Option<u64>,
        image_name: Option<&str>,
    ) -> DecodedProcessEvent {
        DecodedProcessEvent {
            event_id,
            version: 0,
            pid,
            create_time,
            parent_pid: None,
            sequence,
            image_name: image_name.map(ToOwned::to_owned),
            image_name_state: if image_name.is_some() {
                FieldState::Present
            } else {
                FieldState::Missing
            },
            partial: false,
        }
    }

    fn start(pid: u32, create_time: u64, sequence: Option<u64>) -> DecodedProcessEvent {
        event(
            EVENT_PROCESS_START,
            pid,
            create_time,
            sequence,
            Some("cmd.exe"),
        )
    }

    fn stop(pid: u32, create_time: u64, sequence: Option<u64>) -> DecodedProcessEvent {
        event(
            EVENT_PROCESS_STOP,
            pid,
            create_time,
            sequence,
            Some("cmd.exe"),
        )
    }

    #[test]
    fn pairs_start_and_stop_by_identity() {
        let mut state = PairingState::new();
        assert_eq!(
            state.observe(&start(10, 100, Some(7)), 1),
            PairingOutcome::StartOpened
        );
        assert_eq!(
            state.observe(&stop(10, 100, Some(7)), 2),
            PairingOutcome::StopPaired {
                start_event_sequence: 1,
                sequence_match: Some(true)
            }
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 1);
        assert_eq!(snapshot.starts_seen, 1);
        assert_eq!(snapshot.stops_seen, 1);
        assert_eq!(snapshot.unpaired_starts, 0);
        assert_eq!(snapshot.orphan_stops, 0);
        assert_eq!(snapshot.sequence_mismatches, 0);
        assert!(!snapshot.has_integrity_issue());
    }

    #[test]
    fn orphan_stop_is_counted_and_retained() {
        let mut state = PairingState::new();
        assert_eq!(
            state.observe(&stop(10, 100, Some(7)), 1),
            PairingOutcome::StopOrphan { retained: true }
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 0);
        assert_eq!(snapshot.orphan_stops, 1);
        assert_eq!(snapshot.pending_orphan_stops, 1);
        assert!(!snapshot.has_integrity_issue());
    }

    #[test]
    fn unpaired_start_is_reported_at_finish() {
        let mut state = PairingState::new();
        state.observe(&start(10, 100, Some(7)), 1);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 0);
        assert_eq!(snapshot.unpaired_starts, 1);
        assert_eq!(state.open_start_samples(8).len(), 1);
        assert!(!snapshot.has_integrity_issue());
    }

    #[test]
    fn sequential_pid_reuse_pairs_each_generation() {
        let mut state = PairingState::new();
        state.observe(&start(10, 100, Some(1)), 1);
        state.observe(&stop(10, 100, Some(1)), 2);
        state.observe(&start(10, 200, Some(2)), 3);
        state.observe(&stop(10, 200, Some(2)), 4);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 2);
        assert_eq!(snapshot.pid_reuse_detected, 0);
        assert_eq!(snapshot.unpaired_starts, 0);
        assert_eq!(snapshot.sequence_mismatches, 0);
    }

    #[test]
    fn overlapping_pid_reuse_is_detected_without_cross_pairing() {
        let mut state = PairingState::new();
        state.observe(&start(10, 100, Some(1)), 1);
        state.observe(&start(10, 200, Some(2)), 2);

        assert_eq!(state.snapshot().pid_reuse_detected, 1);
        assert_eq!(
            state.observe(&stop(10, 100, Some(1)), 3),
            PairingOutcome::StopPaired {
                start_event_sequence: 1,
                sequence_match: Some(true)
            }
        );
        assert_eq!(
            state.observe(&stop(10, 200, Some(2)), 4),
            PairingOutcome::StopPaired {
                start_event_sequence: 2,
                sequence_match: Some(true)
            }
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 2);
        assert_eq!(snapshot.unpaired_starts, 0);
        assert!(snapshot.has_integrity_issue());
    }

    #[test]
    fn sequence_mismatch_is_flagged_but_identity_still_pairs() {
        let mut state = PairingState::new();
        state.observe(&start(10, 100, Some(1)), 1);
        assert_eq!(
            state.observe(&stop(10, 100, Some(2)), 2),
            PairingOutcome::StopPaired {
                start_event_sequence: 1,
                sequence_match: Some(false)
            }
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 1);
        assert_eq!(snapshot.sequence_mismatches, 1);
        assert!(snapshot.has_integrity_issue());
    }

    #[test]
    fn missing_sequence_is_not_a_mismatch() {
        let mut state = PairingState::new();
        state.observe(&start(10, 100, Some(1)), 1);
        assert_eq!(
            state.observe(&stop(10, 100, None), 2),
            PairingOutcome::StopPaired {
                start_event_sequence: 1,
                sequence_match: None
            }
        );
        assert_eq!(state.snapshot().sequence_mismatches, 0);
        assert!(!state.snapshot().has_integrity_issue());
    }

    #[test]
    fn duplicate_start_is_counted() {
        let mut state = PairingState::new();
        assert_eq!(
            state.observe(&start(10, 100, Some(1)), 1),
            PairingOutcome::StartOpened
        );
        assert_eq!(
            state.observe(&start(10, 100, Some(1)), 2),
            PairingOutcome::StartDuplicate {
                first_event_sequence: 1
            }
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.starts_seen, 2);
        assert_eq!(snapshot.duplicate_starts, 1);
        assert_eq!(snapshot.unpaired_starts, 1);
        assert!(snapshot.has_integrity_issue());
    }

    #[test]
    fn duplicate_stop_is_counted_after_the_pair_closed() {
        let mut state = PairingState::new();
        state.observe(&start(10, 100, Some(1)), 1);
        state.observe(&stop(10, 100, Some(1)), 2);
        assert_eq!(
            state.observe(&stop(10, 100, Some(1)), 3),
            PairingOutcome::StopDuplicate {
                first_event_sequence: 2
            }
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 1);
        assert_eq!(snapshot.duplicate_stops, 1);
        assert!(snapshot.has_integrity_issue());
    }

    #[test]
    fn duplicate_stop_is_counted_for_a_retained_orphan() {
        let mut state = PairingState::new();
        state.observe(&stop(10, 100, Some(1)), 1);
        assert_eq!(
            state.observe(&stop(10, 100, Some(1)), 2),
            PairingOutcome::StopDuplicate {
                first_event_sequence: 1
            }
        );
        assert_eq!(state.snapshot().duplicate_stops, 1);
        assert!(state.snapshot().has_integrity_issue());
    }

    #[test]
    fn late_start_pairs_an_earlier_orphan_stop() {
        let mut state = PairingState::new();
        state.observe(&stop(10, 100, Some(1)), 1);
        assert_eq!(
            state.observe(&start(10, 100, Some(1)), 2),
            PairingOutcome::StartPairedEarlierStop {
                stop_event_sequence: 1,
                sequence_match: Some(true)
            }
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 0);
        assert_eq!(snapshot.out_of_order_pairs, 1);
        assert_eq!(snapshot.pending_orphan_stops, 0);
        assert_eq!(snapshot.duplicate_stops, 0);
        assert!(snapshot.has_integrity_issue());
    }

    #[test]
    fn stop_for_a_reused_pid_never_cross_pairs() {
        let mut state = PairingState::new();
        state.observe(&start(10, 200, Some(2)), 1);
        assert_eq!(
            state.observe(&stop(10, 100, Some(1)), 2),
            PairingOutcome::StopIdentityMismatch {
                open_create_time: 200,
                retained: true
            }
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 0);
        assert_eq!(snapshot.identity_mismatched_stops, 1);
        assert_eq!(snapshot.unpaired_starts, 1);
        assert_eq!(snapshot.pending_orphan_stops, 1);
        assert!(!snapshot.has_integrity_issue());

        // The real old process start arrives later and resolves the stop.
        assert_eq!(
            state.observe(&start(10, 100, Some(1)), 3),
            PairingOutcome::StartPairedEarlierStop {
                stop_event_sequence: 2,
                sequence_match: Some(true)
            }
        );
        assert_eq!(state.snapshot().pending_orphan_stops, 0);
    }

    #[test]
    fn zero_create_time_is_indeterminate_and_never_paired() {
        let mut state = PairingState::new();
        assert_eq!(
            state.observe(&start(10, 0, Some(1)), 1),
            PairingOutcome::IdentityIndeterminate
        );
        assert_eq!(
            state.observe(&stop(10, 0, Some(1)), 2),
            PairingOutcome::IdentityIndeterminate
        );

        let snapshot = state.snapshot();
        assert_eq!(snapshot.paired, 0);
        assert_eq!(snapshot.indeterminate_identities, 2);
        assert_eq!(snapshot.starts_seen, 0);
        assert!(snapshot.has_integrity_issue());
    }

    #[test]
    fn observed_start_image_names_are_deduplicated_and_bounded() {
        let mut state = PairingState::new();
        for index in 0..=MAX_OBSERVED_START_IMAGES {
            let image = format!("process-{index}.exe");
            state.observe(
                &event(
                    EVENT_PROCESS_START,
                    index as u32 + 1,
                    100,
                    Some(index as u64),
                    Some(&image),
                ),
                index as u64 + 1,
            );
        }
        state.observe(
            &event(
                EVENT_PROCESS_START,
                9999,
                100,
                Some(9999),
                Some("process-0.exe"),
            ),
            10_000,
        );

        let snapshot = state.snapshot();
        assert_eq!(
            snapshot.observed_start_image_names.len(),
            MAX_OBSERVED_START_IMAGES
        );
        assert!(snapshot.observed_start_image_names_truncated);
        assert_eq!(
            snapshot
                .observed_start_image_names
                .iter()
                .filter(|name| name.as_str() == "process-0.exe")
                .count(),
            1
        );
    }

    #[test]
    fn observed_start_image_is_basename_only() {
        let mut state = PairingState::new();
        state.observe(
            &event(
                EVENT_PROCESS_START,
                1,
                100,
                Some(1),
                Some(r"C:\Windows\System32\cmd.exe"),
            ),
            1,
        );

        let snapshot = state.snapshot();
        assert_eq!(
            snapshot.observed_start_image_names,
            vec!["cmd.exe".to_owned()]
        );
    }

    #[test]
    fn stop_only_window_has_a_coverage_issue() {
        let mut state = PairingState::new();
        state.observe(&stop(1, 100, None), 1);
        assert!(state.snapshot().coverage_issue().is_some());

        state.observe(&start(2, 100, None), 2);
        assert!(state.snapshot().coverage_issue().is_none());
    }

    #[test]
    fn start_overflow_is_bounded_and_flagged() {
        let mut state = PairingState::new();
        for index in 0..=MAX_OPEN_STARTS {
            let outcome = state.observe(
                &event(
                    EVENT_PROCESS_START,
                    index as u32 + 1,
                    100,
                    Some(index as u64),
                    None,
                ),
                index as u64 + 1,
            );
            if index == MAX_OPEN_STARTS {
                assert_eq!(outcome, PairingOutcome::StartOverflow);
            }
        }

        let snapshot = state.snapshot();
        assert_eq!(snapshot.start_overflow, 1);
        assert_eq!(snapshot.starts_seen, MAX_OPEN_STARTS as u64 + 1);
        assert_eq!(snapshot.unpaired_starts, MAX_OPEN_STARTS as u64);
        assert!(snapshot.has_integrity_issue());
    }

    #[test]
    fn pending_orphan_stops_are_bounded_and_flagged() {
        let mut state = PairingState::new();
        for index in 0..=MAX_PENDING_ORPHAN_STOPS {
            let outcome = state.observe(
                &stop(index as u32 + 1, 100, Some(index as u64)),
                index as u64 + 1,
            );
            if index == MAX_PENDING_ORPHAN_STOPS {
                assert_eq!(outcome, PairingOutcome::StopOrphan { retained: false });
            }
        }

        let snapshot = state.snapshot();
        assert_eq!(snapshot.orphan_stop_overflow, 1);
        assert_eq!(snapshot.orphan_stops, MAX_PENDING_ORPHAN_STOPS as u64 + 1);
        assert_eq!(
            snapshot.pending_orphan_stops,
            MAX_PENDING_ORPHAN_STOPS as u64
        );
        assert!(snapshot.has_integrity_issue());
    }

    #[test]
    fn open_start_samples_are_bounded() {
        let mut state = PairingState::new();
        for index in 0..MAX_OPEN_START_SAMPLES + 5 {
            state.observe(
                &event(
                    EVENT_PROCESS_START,
                    index as u32 + 1,
                    100,
                    Some(index as u64),
                    None,
                ),
                index as u64 + 1,
            );
        }
        assert_eq!(state.open_start_samples(1000).len(), MAX_OPEN_START_SAMPLES);
    }

    #[test]
    fn observed_image_name_is_length_bounded() {
        let mut state = PairingState::new();
        let long_name = "x".repeat(MAX_OBSERVED_IMAGE_NAME_CHARS + 10);
        state.observe(
            &event(EVENT_PROCESS_START, 1, 100, Some(1), Some(&long_name)),
            1,
        );

        let snapshot = state.snapshot();
        assert_eq!(
            snapshot.observed_start_image_names[0].chars().count(),
            MAX_OBSERVED_IMAGE_NAME_CHARS
        );
        assert!(snapshot.observed_start_image_names_truncated);
    }
}
