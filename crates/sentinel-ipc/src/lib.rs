//! Local read-only IPC between the SentinelLite app and the sensor.

pub mod client;
pub mod event_server;
pub mod events;
pub mod protocol;
pub mod server;

mod win;

/// Default status-query endpoint. The server creates it with a DACL that only
/// allows the launching user plus SYSTEM/Administrators.
pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\SentinelLiteSensor.Status.v1";

/// Default normalized-event endpoint. Same DACL and client authentication as
/// the status endpoint; kept separate so event batches cannot delay health
/// queries.
pub const DEFAULT_EVENT_PIPE_NAME: &str = r"\\.\pipe\SentinelLiteSensor.Events.v1";
