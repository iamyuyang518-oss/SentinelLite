//! Versioned, bounded status-query protocol.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_REQUEST_BYTES: usize = 512;
pub const MAX_RESPONSE_BYTES: usize = 16 * 1024;
pub const MAX_ERROR_CHARS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensorState {
    NotStarted,
    Starting,
    Active,
    Degraded,
    Failed,
    Stopped,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    Unknown,
    Granted,
    Limited,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensorStatus {
    pub sensor_state: SensorState,
    pub permission_state: PermissionState,
    pub events_received: u64,
    pub events_processed: u64,
    pub events_dropped: u64,
    pub events_lost: Option<u64>,
    pub realtime_buffers_lost: Option<u64>,
    pub decode_errors: u64,
    pub last_event_at: Option<String>,
    pub uptime_ms: u64,
    pub health_ok: bool,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusRequest {
    pub version: u16,
    pub request: RequestKind,
}

impl StatusRequest {
    pub fn status() -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request: RequestKind::Status,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCode {
    UnsupportedVersion,
    InvalidRequest,
    RequestTooLarge,
    ResponseTooLarge,
    Unauthorized,
    Internal,
    Timeout,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtocolError {
    pub code: ProtocolErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn new(code: ProtocolErrorCode, message: impl Into<String>) -> Self {
        let message: String = message.into().chars().take(MAX_ERROR_CHARS).collect();
        Self { code, message }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusResponse {
    pub version: u16,
    pub ok: bool,
    pub status: Option<SensorStatus>,
    pub error: Option<ProtocolError>,
}

impl StatusResponse {
    pub fn success(status: SensorStatus) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            ok: true,
            status: Some(status),
            error: None,
        }
    }

    pub fn failure(code: ProtocolErrorCode, message: impl Into<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            ok: false,
            status: None,
            error: Some(ProtocolError::new(code, message)),
        }
    }
}

pub fn encode_request(request: &StatusRequest) -> Result<Vec<u8>, ProtocolError> {
    validate_request(request)?;
    let bytes = serde_json::to_vec(request).map_err(|error| {
        ProtocolError::new(ProtocolErrorCode::InvalidRequest, error.to_string())
    })?;
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::RequestTooLarge,
            "request message exceeds limit",
        ));
    }
    Ok(bytes)
}

pub fn decode_request(bytes: &[u8]) -> Result<StatusRequest, ProtocolError> {
    if bytes.len() > MAX_REQUEST_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::RequestTooLarge,
            "request message exceeds limit",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "request is not valid UTF-8",
        )
    })?;
    let request: StatusRequest = serde_json::from_str(text).map_err(|_| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "request is not valid protocol JSON",
        )
    })?;
    validate_request(&request)?;
    Ok(request)
}

fn validate_request(request: &StatusRequest) -> Result<(), ProtocolError> {
    if request.version != PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            ProtocolErrorCode::UnsupportedVersion,
            "unsupported protocol version",
        ));
    }
    Ok(())
}

pub fn encode_response(response: &StatusResponse) -> Result<Vec<u8>, ProtocolError> {
    if response.version != PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            ProtocolErrorCode::UnsupportedVersion,
            "unsupported protocol version",
        ));
    }
    validate_response_shape(response)?;
    let bytes = serde_json::to_vec(response)
        .map_err(|error| ProtocolError::new(ProtocolErrorCode::Internal, error.to_string()))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::ResponseTooLarge,
            "response message exceeds limit",
        ));
    }
    Ok(bytes)
}

pub fn decode_response(bytes: &[u8]) -> Result<StatusResponse, ProtocolError> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(ProtocolError::new(
            ProtocolErrorCode::ResponseTooLarge,
            "response message exceeds limit",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "response is not valid UTF-8",
        )
    })?;
    let response: StatusResponse = serde_json::from_str(text).map_err(|_| {
        ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "response is not valid protocol JSON",
        )
    })?;
    if response.version != PROTOCOL_VERSION {
        return Err(ProtocolError::new(
            ProtocolErrorCode::UnsupportedVersion,
            "unsupported protocol version",
        ));
    }
    validate_response_shape(&response)?;
    Ok(response)
}

fn validate_response_shape(response: &StatusResponse) -> Result<(), ProtocolError> {
    let valid = matches!(
        (
            response.ok,
            response.status.is_some(),
            response.error.is_some()
        ),
        (true, true, false) | (false, false, true)
    );
    if valid {
        Ok(())
    } else {
        Err(ProtocolError::new(
            ProtocolErrorCode::InvalidRequest,
            "response status/error fields are inconsistent",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_status() -> SensorStatus {
        SensorStatus {
            sensor_state: SensorState::Active,
            permission_state: PermissionState::Granted,
            events_received: 12,
            events_processed: 10,
            events_dropped: 1,
            events_lost: Some(0),
            realtime_buffers_lost: Some(0),
            decode_errors: 1,
            last_event_at: Some("2026-10-31T00:00:00.000Z".to_owned()),
            uptime_ms: 42_000,
            health_ok: false,
            last_error: Some("partial event".to_owned()),
        }
    }

    #[test]
    fn valid_status_request_round_trips() {
        let request = StatusRequest::status();
        let bytes = encode_request(&request).expect("request should encode");
        let decoded = decode_request(&bytes).expect("request should decode");
        assert_eq!(decoded, request);
        assert_eq!(decoded.version, PROTOCOL_VERSION);
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": PROTOCOL_VERSION + 1,
            "request": "status"
        }))
        .unwrap();
        let error = decode_request(&bytes).expect_err("wrong version must fail");
        assert_eq!(error.code, ProtocolErrorCode::UnsupportedVersion);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": PROTOCOL_VERSION,
            "request": "status",
            "extra": true
        }))
        .unwrap();
        let error = decode_request(&bytes).expect_err("unknown field must fail");
        assert_eq!(error.code, ProtocolErrorCode::InvalidRequest);
    }

    #[test]
    fn oversized_request_is_rejected_without_parsing() {
        let bytes = vec![b'x'; MAX_REQUEST_BYTES + 1];
        let error = decode_request(&bytes).expect_err("oversized request must fail");
        assert_eq!(error.code, ProtocolErrorCode::RequestTooLarge);
    }

    #[test]
    fn invalid_utf8_request_is_rejected() {
        let error = decode_request(&[0xff, 0xfe]).expect_err("invalid UTF-8 must fail");
        assert_eq!(error.code, ProtocolErrorCode::InvalidRequest);
    }

    #[test]
    fn success_response_round_trips() {
        let response = StatusResponse::success(sample_status());
        let bytes = encode_response(&response).expect("response should encode");
        let decoded = decode_response(&bytes).expect("response should decode");
        assert_eq!(decoded, response);
        assert!(decoded.ok);
        assert!(decoded.error.is_none());
    }

    #[test]
    fn error_response_round_trips() {
        let response =
            StatusResponse::failure(ProtocolErrorCode::Internal, "sensor status unavailable");
        let bytes = encode_response(&response).expect("response should encode");
        let decoded = decode_response(&bytes).expect("response should decode");
        assert_eq!(decoded, response);
        assert!(!decoded.ok);
        assert!(decoded.status.is_none());
    }

    #[test]
    fn success_response_without_status_is_rejected() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": PROTOCOL_VERSION,
            "ok": true,
            "status": null,
            "error": null
        }))
        .unwrap();
        let error = decode_response(&bytes).expect_err("success without status must fail");
        assert_eq!(error.code, ProtocolErrorCode::InvalidRequest);
    }

    #[test]
    fn oversized_response_is_rejected() {
        let mut status = sample_status();
        status.last_error = Some("x".repeat(MAX_RESPONSE_BYTES));
        let error = encode_response(&StatusResponse::success(status))
            .expect_err("oversized response must fail");
        assert_eq!(error.code, ProtocolErrorCode::ResponseTooLarge);
    }
}
