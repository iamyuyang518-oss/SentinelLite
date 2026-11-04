//! Normalized event model.
//!
//! The definitions live in `sentinel-events` so IPC, storage and behavior
//! analysis share exactly the same wire and persistence types. This module is
//! kept as the sensor's stable import path.

pub use sentinel_events::model::*;
