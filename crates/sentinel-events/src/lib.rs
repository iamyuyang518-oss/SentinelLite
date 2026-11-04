//! Shared, serializable process event model.
//!
//! The sensor produces these records, IPC transports them, storage persists
//! them, and behavior analysis consumes them. Keeping one definition avoids
//! the sensor, IPC and application drifting into different event semantics.

pub mod model;
pub mod time;

pub use model::*;
