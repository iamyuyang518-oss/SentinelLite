//! Schema-driven decoding for `Microsoft-Windows-Kernel-Process` events.
//!
//! The payload layout is packed (no alignment padding). The field order and
//! types below were verified against the Windows 11 26200 TDH metadata for
//! this host. Only fields consumed by the Phase 2.1 slice are listed; fields
//! after `ImageName` are intentionally not decoded.

use std::fmt;

use windows_sys::Win32::Globalization::{CP_ACP, MB_ERR_INVALID_CHARS, MultiByteToWideChar};

pub use sentinel_events::FieldState;

pub const EVENT_PROCESS_START: u16 = 1;
pub const EVENT_PROCESS_STOP: u16 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedProcessEvent {
    pub event_id: u16,
    pub version: u8,
    pub pid: u32,
    pub create_time: u64,
    pub parent_pid: Option<u32>,
    pub sequence: Option<u64>,
    pub image_name: Option<String>,
    pub image_name_state: FieldState,
    pub partial: bool,
}

impl DecodedProcessEvent {
    pub fn is_start(&self) -> bool {
        self.event_id == EVENT_PROCESS_START
    }

    pub fn is_stop(&self) -> bool {
        self.event_id == EVENT_PROCESS_STOP
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    UnsupportedEvent {
        event_id: u16,
    },
    UnsupportedVersion {
        event_id: u16,
        version: u8,
    },
    Truncated {
        event_id: u16,
        version: u8,
        field: &'static str,
        offset: usize,
        needed: usize,
        available: usize,
    },
    InvalidSid {
        event_id: u16,
        version: u8,
        field: &'static str,
        sub_authority_count: u8,
    },
    MissingField {
        field: &'static str,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEvent { event_id } => {
                write!(f, "unsupported event id {event_id}")
            }
            Self::UnsupportedVersion { event_id, version } => {
                write!(f, "unsupported event {event_id} version {version}")
            }
            Self::Truncated {
                event_id,
                version,
                field,
                offset,
                needed,
                available,
            } => write!(
                f,
                "event {event_id} v{version} truncated at {field} (offset {offset}, need {needed}, have {available})"
            ),
            Self::InvalidSid {
                event_id,
                version,
                field,
                sub_authority_count,
            } => write!(
                f,
                "event {event_id} v{version} has invalid SID in {field} (sub-authority count {sub_authority_count})"
            ),
            Self::MissingField { field } => write!(f, "missing required field {field}"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Clone, Copy)]
enum FieldKind {
    U32,
    U64,
    Utf16,
    Ansi,
    Sid,
}

#[derive(Debug, Clone, Copy)]
struct Field {
    name: &'static str,
    kind: FieldKind,
}

const START_V0: &[Field] = &[
    Field {
        name: "ProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "CreateTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ParentProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "SessionID",
        kind: FieldKind::U32,
    },
    Field {
        name: "ImageName",
        kind: FieldKind::Utf16,
    },
];

const START_V1: &[Field] = &[
    Field {
        name: "ProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "CreateTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ParentProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "SessionID",
        kind: FieldKind::U32,
    },
    Field {
        name: "Flags",
        kind: FieldKind::U32,
    },
    Field {
        name: "ImageName",
        kind: FieldKind::Utf16,
    },
];

const START_V2: &[Field] = &[
    Field {
        name: "ProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "CreateTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ParentProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "SessionID",
        kind: FieldKind::U32,
    },
    Field {
        name: "Flags",
        kind: FieldKind::U32,
    },
    Field {
        name: "ImageName",
        kind: FieldKind::Utf16,
    },
];

const START_V3_V4: &[Field] = &[
    Field {
        name: "ProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "ProcessSequenceNumber",
        kind: FieldKind::U64,
    },
    Field {
        name: "CreateTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ParentProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "ParentProcessSequenceNumber",
        kind: FieldKind::U64,
    },
    Field {
        name: "SessionID",
        kind: FieldKind::U32,
    },
    Field {
        name: "Flags",
        kind: FieldKind::U32,
    },
    Field {
        name: "ProcessTokenElevationType",
        kind: FieldKind::U32,
    },
    Field {
        name: "ProcessTokenIsElevated",
        kind: FieldKind::U32,
    },
    Field {
        name: "MandatoryLabel",
        kind: FieldKind::Sid,
    },
    Field {
        name: "ImageName",
        kind: FieldKind::Utf16,
    },
];

const STOP_V0: &[Field] = &[
    Field {
        name: "ProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "CreateTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ExitTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ExitCode",
        kind: FieldKind::U32,
    },
    Field {
        name: "TokenElevationType",
        kind: FieldKind::U32,
    },
    Field {
        name: "HandleCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "CommitCharge",
        kind: FieldKind::U64,
    },
    Field {
        name: "CommitPeak",
        kind: FieldKind::U64,
    },
    Field {
        name: "ImageName",
        kind: FieldKind::Ansi,
    },
];

const STOP_V1: &[Field] = &[
    Field {
        name: "ProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "CreateTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ExitTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ExitCode",
        kind: FieldKind::U32,
    },
    Field {
        name: "TokenElevationType",
        kind: FieldKind::U32,
    },
    Field {
        name: "HandleCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "CommitCharge",
        kind: FieldKind::U64,
    },
    Field {
        name: "CommitPeak",
        kind: FieldKind::U64,
    },
    Field {
        name: "CPUCycleCount",
        kind: FieldKind::U64,
    },
    Field {
        name: "ReadOperationCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "WriteOperationCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "ReadTransferKiloBytes",
        kind: FieldKind::U32,
    },
    Field {
        name: "WriteTransferKiloBytes",
        kind: FieldKind::U32,
    },
    Field {
        name: "HardFaultCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "ImageName",
        kind: FieldKind::Ansi,
    },
];

const STOP_V2: &[Field] = &[
    Field {
        name: "ProcessID",
        kind: FieldKind::U32,
    },
    Field {
        name: "ProcessSequenceNumber",
        kind: FieldKind::U64,
    },
    Field {
        name: "CreateTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ExitTime",
        kind: FieldKind::U64,
    },
    Field {
        name: "ExitCode",
        kind: FieldKind::U32,
    },
    Field {
        name: "TokenElevationType",
        kind: FieldKind::U32,
    },
    Field {
        name: "HandleCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "CommitCharge",
        kind: FieldKind::U64,
    },
    Field {
        name: "CommitPeak",
        kind: FieldKind::U64,
    },
    Field {
        name: "CPUCycleCount",
        kind: FieldKind::U64,
    },
    Field {
        name: "ReadOperationCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "WriteOperationCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "ReadTransferKiloBytes",
        kind: FieldKind::U32,
    },
    Field {
        name: "WriteTransferKiloBytes",
        kind: FieldKind::U32,
    },
    Field {
        name: "HardFaultCount",
        kind: FieldKind::U32,
    },
    Field {
        name: "ImageName",
        kind: FieldKind::Ansi,
    },
];

fn schema(event_id: u16, version: u8) -> Option<&'static [Field]> {
    match (event_id, version) {
        (EVENT_PROCESS_START, 0) => Some(START_V0),
        (EVENT_PROCESS_START, 1) => Some(START_V1),
        (EVENT_PROCESS_START, 2) => Some(START_V2),
        (EVENT_PROCESS_START, 3 | 4) => Some(START_V3_V4),
        (EVENT_PROCESS_STOP, 0) => Some(STOP_V0),
        (EVENT_PROCESS_STOP, 1) => Some(STOP_V1),
        (EVENT_PROCESS_STOP, 2) => Some(STOP_V2),
        _ => None,
    }
}

pub fn decode_process_event(
    event_id: u16,
    version: u8,
    payload: &[u8],
) -> Result<DecodedProcessEvent, DecodeError> {
    if event_id != EVENT_PROCESS_START && event_id != EVENT_PROCESS_STOP {
        return Err(DecodeError::UnsupportedEvent { event_id });
    }
    let schema =
        schema(event_id, version).ok_or(DecodeError::UnsupportedVersion { event_id, version })?;

    let mut cursor = Cursor::new(payload, event_id, version);
    let mut pid = None;
    let mut create_time = None;
    let mut parent_pid = None;
    let mut sequence = None;
    let mut image_name = None;
    let mut image_name_state = FieldState::Missing;

    for field in schema {
        match field.kind {
            FieldKind::U32 => {
                let value = cursor.read_u32(field.name)?;
                match field.name {
                    "ProcessID" => pid = Some(value),
                    "ParentProcessID" => parent_pid = Some(value),
                    _ => {}
                }
            }
            FieldKind::U64 => {
                let value = cursor.read_u64(field.name)?;
                match field.name {
                    "CreateTime" => create_time = Some(value),
                    "ProcessSequenceNumber" => sequence = Some(value),
                    _ => {}
                }
            }
            FieldKind::Utf16 | FieldKind::Ansi => {
                let (value, state) = cursor.read_string(field.kind, field.name)?;
                if field.name == "ImageName" {
                    image_name = value;
                    image_name_state = state;
                }
            }
            FieldKind::Sid => cursor.skip_sid(field.name)?,
        }
    }

    let pid = pid.ok_or(DecodeError::MissingField { field: "ProcessID" })?;
    let create_time = create_time.ok_or(DecodeError::MissingField {
        field: "CreateTime",
    })?;

    Ok(DecodedProcessEvent {
        event_id,
        version,
        pid,
        create_time,
        parent_pid,
        sequence,
        image_name,
        image_name_state,
        partial: image_name_state == FieldState::Partial,
    })
}

struct Cursor<'a> {
    payload: &'a [u8],
    offset: usize,
    event_id: u16,
    version: u8,
}

impl<'a> Cursor<'a> {
    fn new(payload: &'a [u8], event_id: u16, version: u8) -> Self {
        Self {
            payload,
            offset: 0,
            event_id,
            version,
        }
    }

    fn remaining(&self) -> usize {
        self.payload.len().saturating_sub(self.offset)
    }

    fn truncated(&self, field: &'static str, needed: usize) -> DecodeError {
        DecodeError::Truncated {
            event_id: self.event_id,
            version: self.version,
            field,
            offset: self.offset,
            needed,
            available: self.remaining(),
        }
    }

    fn take(&mut self, field: &'static str, needed: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .offset
            .checked_add(needed)
            .ok_or_else(|| self.truncated(field, needed))?;
        if end > self.payload.len() {
            return Err(self.truncated(field, needed));
        }
        let value = &self.payload[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn read_u32(&mut self, field: &'static str) -> Result<u32, DecodeError> {
        let bytes = self.take(field, 4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self, field: &'static str) -> Result<u64, DecodeError> {
        let bytes = self.take(field, 8)?;
        Ok(u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn read_string(
        &mut self,
        kind: FieldKind,
        field: &'static str,
    ) -> Result<(Option<String>, FieldState), DecodeError> {
        match kind {
            FieldKind::Utf16 => self.read_utf16(field),
            FieldKind::Ansi => self.read_ansi(field),
            FieldKind::U32 | FieldKind::U64 | FieldKind::Sid => {
                unreachable!("string reader called for non-string field")
            }
        }
    }

    fn read_utf16(
        &mut self,
        _field: &'static str,
    ) -> Result<(Option<String>, FieldState), DecodeError> {
        let start = self.offset;
        if start >= self.payload.len() {
            return Ok((None, FieldState::Missing));
        }
        if self.remaining() < 2 {
            self.offset = self.payload.len();
            return Ok((None, FieldState::Partial));
        }

        let mut terminator = None;
        let mut index = start;
        while index + 1 < self.payload.len() {
            if self.payload[index] == 0 && self.payload[index + 1] == 0 {
                terminator = Some(index);
                break;
            }
            index += 2;
        }

        let units_end = match terminator {
            Some(index) => index,
            None => start + (self.remaining() / 2) * 2,
        };
        let mut units = Vec::with_capacity(units_end.saturating_sub(start) / 2);
        let mut index = start;
        while index < units_end {
            units.push(u16::from_le_bytes([
                self.payload[index],
                self.payload[index + 1],
            ]));
            index += 2;
        }

        self.offset = match terminator {
            Some(index) => index + 2,
            None => self.payload.len(),
        };

        let terminated = terminator.is_some();
        let (value, valid) = match String::from_utf16(&units) {
            Ok(value) => (value, true),
            Err(_) => (String::from_utf16_lossy(&units), false),
        };
        let state = if terminated && valid {
            FieldState::Present
        } else {
            FieldState::Partial
        };
        Ok((Some(value), state))
    }

    fn read_ansi(
        &mut self,
        _field: &'static str,
    ) -> Result<(Option<String>, FieldState), DecodeError> {
        let start = self.offset;
        if start >= self.payload.len() {
            return Ok((None, FieldState::Missing));
        }
        match self.payload[start..].iter().position(|byte| *byte == 0) {
            Some(relative) => {
                let end = start + relative;
                let bytes = &self.payload[start..end];
                self.offset = end + 1;
                let (value, valid) = decode_ansi_bytes(bytes);
                let state = if valid {
                    FieldState::Present
                } else {
                    FieldState::Partial
                };
                Ok((Some(value), state))
            }
            None => {
                let bytes = &self.payload[start..];
                self.offset = self.payload.len();
                let (value, _) = decode_ansi_bytes(bytes);
                Ok((Some(value), FieldState::Partial))
            }
        }
    }

    fn skip_sid(&mut self, field: &'static str) -> Result<(), DecodeError> {
        let start = self.offset;
        if self.remaining() < 8 {
            return Err(self.truncated(field, 8));
        }
        let sub_authority_count = self.payload[start + 1];
        if sub_authority_count > 15 {
            return Err(DecodeError::InvalidSid {
                event_id: self.event_id,
                version: self.version,
                field,
                sub_authority_count,
            });
        }
        let total = 8 + 4 * usize::from(sub_authority_count);
        self.take(field, total)?;
        Ok(())
    }
}

/// Decode a `win:AnsiString` using the system ANSI code page (CP_ACP), not
/// UTF-8. The returned flag is false when the OS decoder rejects the bytes.
fn decode_ansi_bytes(bytes: &[u8]) -> (String, bool) {
    if bytes.is_empty() {
        return (String::new(), true);
    }
    if bytes.is_ascii() {
        return (String::from_utf8_lossy(bytes).into_owned(), true);
    }

    let Ok(byte_count) = i32::try_from(bytes.len()) else {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    };
    // SAFETY: the input pointer is valid for byte_count bytes and the output
    // length query uses a null buffer.
    let wide_len = unsafe {
        MultiByteToWideChar(
            CP_ACP,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            byte_count,
            std::ptr::null_mut(),
            0,
        )
    };
    if wide_len <= 0 {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }

    let mut wide = vec![0u16; wide_len as usize];
    // SAFETY: wide is allocated for wide_len UTF-16 code units.
    let written = unsafe {
        MultiByteToWideChar(
            CP_ACP,
            MB_ERR_INVALID_CHARS,
            bytes.as_ptr(),
            byte_count,
            wide.as_mut_ptr(),
            wide_len,
        )
    };
    if written <= 0 {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }
    wide.truncate(written as usize);
    (String::from_utf16_lossy(&wide), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn put_u64(out: &mut Vec<u8>, value: u64) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn put_utf16(out: &mut Vec<u8>, value: &str) {
        for unit in value.encode_utf16() {
            out.extend_from_slice(&unit.to_le_bytes());
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }

    fn put_ansiz(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(value.as_bytes());
        out.push(0);
    }

    fn put_ansi_bytes(out: &mut Vec<u8>, value: &[u8]) {
        out.extend_from_slice(value);
        out.push(0);
    }

    fn put_sid(out: &mut Vec<u8>, sub_authorities: &[u32]) {
        out.push(1);
        out.push(sub_authorities.len() as u8);
        out.extend_from_slice(&[0, 0, 0, 0, 0, 16]);
        for authority in sub_authorities {
            put_u32(out, *authority);
        }
    }

    fn start_v0_payload(image: &str) -> Vec<u8> {
        let mut out = Vec::new();
        put_u32(&mut out, 0x1234);
        put_u64(&mut out, 0x1122_3344_5566_7788);
        put_u32(&mut out, 0x42);
        put_u32(&mut out, 2);
        put_utf16(&mut out, image);
        out
    }

    #[test]
    fn decodes_start_v0() {
        let event = decode_process_event(1, 0, &start_v0_payload(r"C:\Windows\System32\cmd.exe"))
            .expect("start v0 should decode");

        assert!(event.is_start());
        assert_eq!(event.version, 0);
        assert_eq!(event.pid, 0x1234);
        assert_eq!(event.create_time, 0x1122_3344_5566_7788);
        assert_eq!(event.parent_pid, Some(0x42));
        assert_eq!(event.sequence, None);
        assert_eq!(
            event.image_name.as_deref(),
            Some(r"C:\Windows\System32\cmd.exe")
        );
        assert_eq!(event.image_name_state, FieldState::Present);
        assert!(!event.partial);
    }

    #[test]
    fn decodes_start_v4_with_sid_and_long_name() {
        let long_name = format!(r"\Device\HarddiskVolume4\Temp\{}-cmd.exe", "x".repeat(200));
        let mut payload = Vec::new();
        put_u32(&mut payload, 0x2222);
        put_u64(&mut payload, 0xAAAA_BBBB_CCCC_DDDD);
        put_u64(&mut payload, 0x0102_0304_0506_0708);
        put_u32(&mut payload, 0x1111);
        put_u64(&mut payload, 0x9999_8888_7777_6666);
        put_u32(&mut payload, 1);
        put_u32(&mut payload, 0x10);
        put_u32(&mut payload, 1);
        put_u32(&mut payload, 0);
        put_sid(&mut payload, &[16, 8192]);
        put_utf16(&mut payload, &long_name);

        let event = decode_process_event(1, 4, &payload).expect("start v4 should decode");

        assert_eq!(event.pid, 0x2222);
        assert_eq!(event.create_time, 0x0102_0304_0506_0708);
        assert_eq!(event.parent_pid, Some(0x1111));
        assert_eq!(event.sequence, Some(0xAAAA_BBBB_CCCC_DDDD));
        assert_eq!(event.image_name.as_deref(), Some(long_name.as_str()));
        assert_eq!(event.image_name_state, FieldState::Present);
        assert!(!event.partial);
    }

    #[test]
    fn decodes_stop_v2_with_ansi_name() {
        let mut payload = Vec::new();
        put_u32(&mut payload, 0x3333);
        put_u64(&mut payload, 0x5555_6666_7777_8888);
        put_u64(&mut payload, 0x0102_0304_0506_0708);
        put_u64(&mut payload, 0x1112_1314_1516_1718);
        put_u32(&mut payload, 0);
        put_u32(&mut payload, 1);
        put_u32(&mut payload, 2);
        put_u64(&mut payload, 3);
        put_u64(&mut payload, 4);
        put_u64(&mut payload, 5);
        put_u32(&mut payload, 6);
        put_u32(&mut payload, 7);
        put_u32(&mut payload, 8);
        put_u32(&mut payload, 9);
        put_u32(&mut payload, 10);
        put_ansiz(&mut payload, "cmd.exe");

        let event = decode_process_event(2, 2, &payload).expect("stop v2 should decode");

        assert!(event.is_stop());
        assert_eq!(event.pid, 0x3333);
        assert_eq!(event.create_time, 0x0102_0304_0506_0708);
        assert_eq!(event.sequence, Some(0x5555_6666_7777_8888));
        assert_eq!(event.image_name.as_deref(), Some("cmd.exe"));
        assert_eq!(event.image_name_state, FieldState::Present);
        assert!(!event.partial);
    }

    #[test]
    fn decodes_stop_ansi_with_system_codepage() {
        // The test host uses CP936; skip on other code pages so the test does
        // not encode locale-specific bytes as a universal expectation.
        // SAFETY: GetACP has no preconditions.
        if unsafe { windows_sys::Win32::Globalization::GetACP() } != 936 {
            return;
        }

        let mut payload = Vec::new();
        put_u32(&mut payload, 0x3333);
        put_u64(&mut payload, 0x5555_6666_7777_8888);
        put_u64(&mut payload, 0x0102_0304_0506_0708);
        put_u64(&mut payload, 0x1112_1314_1516_1718);
        put_u32(&mut payload, 0);
        put_u32(&mut payload, 1);
        put_u32(&mut payload, 2);
        put_u64(&mut payload, 3);
        put_u64(&mut payload, 4);
        put_u64(&mut payload, 5);
        put_u32(&mut payload, 6);
        put_u32(&mut payload, 7);
        put_u32(&mut payload, 8);
        put_u32(&mut payload, 9);
        put_u32(&mut payload, 10);
        // "测试.exe" in CP936.
        put_ansi_bytes(
            &mut payload,
            &[0xB2, 0xE2, 0xCA, 0xD4, 0x2E, 0x65, 0x78, 0x65],
        );

        let event = decode_process_event(2, 2, &payload).expect("stop v2 should decode");
        assert_eq!(event.image_name.as_deref(), Some("测试.exe"));
        assert_eq!(event.image_name_state, FieldState::Present);
        assert!(!event.partial);
    }

    #[test]
    fn truncated_identity_field_is_an_error() {
        let error = decode_process_event(1, 0, &[0x34, 0x12]).expect_err("must fail");
        assert!(matches!(
            error,
            DecodeError::Truncated {
                field: "ProcessID",
                ..
            }
        ));
    }

    #[test]
    fn unterminated_image_name_is_partial_not_fabricated() {
        let mut payload = Vec::new();
        put_u32(&mut payload, 0x1234);
        put_u64(&mut payload, 0x1122_3344_5566_7788);
        put_u32(&mut payload, 0);
        put_u32(&mut payload, 0);
        for unit in "abc".encode_utf16() {
            payload.extend_from_slice(&unit.to_le_bytes());
        }

        let event = decode_process_event(1, 0, &payload).expect("identity is usable");
        assert_eq!(event.image_name.as_deref(), Some("abc"));
        assert_eq!(event.image_name_state, FieldState::Partial);
        assert!(event.partial);
    }

    #[test]
    fn invalid_sid_is_an_error() {
        let mut payload = Vec::new();
        put_u32(&mut payload, 1);
        put_u64(&mut payload, 2);
        put_u64(&mut payload, 3);
        put_u32(&mut payload, 4);
        put_u64(&mut payload, 5);
        put_u32(&mut payload, 6);
        put_u32(&mut payload, 7);
        put_u32(&mut payload, 8);
        put_u32(&mut payload, 9);
        payload.push(1);
        payload.push(20);
        payload.extend_from_slice(&[0; 6]);

        let error = decode_process_event(1, 4, &payload).expect_err("must fail");
        assert!(matches!(error, DecodeError::InvalidSid { .. }));
    }

    #[test]
    fn unknown_event_version_is_an_error() {
        let error = decode_process_event(1, 99, &[]).expect_err("must fail");
        assert!(matches!(
            error,
            DecodeError::UnsupportedVersion {
                event_id: 1,
                version: 99
            }
        ));
    }

    #[test]
    fn unknown_event_id_is_an_error() {
        let error = decode_process_event(5, 0, &[]).expect_err("must fail");
        assert!(matches!(
            error,
            DecodeError::UnsupportedEvent { event_id: 5 }
        ));
    }
}
