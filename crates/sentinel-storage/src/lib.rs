//! Local process-event persistence.
//!
//! The sensor stays stateless; the application-side ingestion worker pulls
//! batches over the authenticated event pipe and writes them to a SQLite
//! database in the application's local data directory. Writes are idempotent
//! by event ID, pairing revisions update the original stop record, and the
//! database is bounded by a retention policy.

pub mod db;
pub mod ingest;
pub mod path;
pub mod retention;

pub use db::{
    EventDetail, EventFilter, EventQueryResult, IngestStats, SessionEventSet, SessionRecord,
    Storage, StorageError, StorageStatus, StoredEvent,
};
pub use ingest::{
    EventSource, IngestionConfig, IngestionHandle, IngestionState, IngestionStatus, IpcEventSource,
};
pub use path::{database_path_in, default_database_path};
pub use retention::RetentionPolicy;
