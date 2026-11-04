use crate::limits::bound_text;
use crate::model::RegistryValueState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegistryValueKind {
    String,
    ExpandString,
    Dword,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRegistryValue<T> {
    pub value: Option<T>,
    pub state: RegistryValueState,
    pub truncated: bool,
}

impl<T> ParsedRegistryValue<T> {
    pub fn read_error() -> Self {
        Self {
            value: None,
            state: RegistryValueState::ReadError,
            truncated: false,
        }
    }
}

pub fn parse_dword(raw: Option<(RegistryValueKind, &[u8])>) -> ParsedRegistryValue<u32> {
    match raw {
        None => ParsedRegistryValue {
            value: None,
            state: RegistryValueState::Missing,
            truncated: false,
        },
        Some((RegistryValueKind::Dword, bytes)) => {
            if bytes.len() != 4 {
                return ParsedRegistryValue {
                    value: None,
                    state: RegistryValueState::Invalid,
                    truncated: false,
                };
            }
            let value = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            ParsedRegistryValue {
                value: Some(value),
                state: RegistryValueState::Present,
                truncated: false,
            }
        }
        Some(_) => ParsedRegistryValue {
            value: None,
            state: RegistryValueState::TypeMismatch,
            truncated: false,
        },
    }
}

pub fn parse_string(
    raw: Option<(RegistryValueKind, &[u8])>,
    max_chars: usize,
) -> ParsedRegistryValue<String> {
    match raw {
        None => ParsedRegistryValue {
            value: None,
            state: RegistryValueState::Missing,
            truncated: false,
        },
        Some((RegistryValueKind::String | RegistryValueKind::ExpandString, bytes)) => {
            if bytes.len() % 2 != 0 {
                return ParsedRegistryValue {
                    value: None,
                    state: RegistryValueState::Invalid,
                    truncated: false,
                };
            }

            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            let mut value = match String::from_utf16(&units) {
                Ok(value) => value,
                Err(_) => {
                    return ParsedRegistryValue {
                        value: None,
                        state: RegistryValueState::Invalid,
                        truncated: false,
                    };
                }
            };

            while value.ends_with('\0') {
                value.pop();
            }
            let bounded = bound_text(&value, max_chars);
            ParsedRegistryValue {
                value: Some(bounded.value),
                state: RegistryValueState::Present,
                truncated: bounded.truncated,
            }
        }
        Some(_) => ParsedRegistryValue {
            value: None,
            state: RegistryValueState::TypeMismatch,
            truncated: false,
        },
    }
}
