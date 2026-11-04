//! Application-side storage and ingestion lifecycle.

use std::sync::{Arc, Mutex};

use sentinel_storage::{
    EventSource, IngestionConfig, IngestionHandle, IngestionState, IngestionStatus, IpcEventSource,
    RetentionPolicy, Storage, StorageStatus, database_path_in,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};

pub struct StorageState {
    storage: Option<Arc<Storage>>,
    ingestion: Mutex<Option<IngestionHandle>>,
    init_error: Option<String>,
    ingestion_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRuntimeReport {
    pub state: &'static str,
    pub init_error: Option<String>,
    pub ingestion_error: Option<String>,
    pub storage: Option<StorageStatus>,
    pub ingestion: IngestionStatus,
    pub last_error: Option<String>,
}

impl StorageState {
    pub fn initialize(app: &AppHandle) -> Self {
        let path = match app.path().app_local_data_dir() {
            Ok(directory) => database_path_in(&directory),
            Err(error) => {
                return Self {
                    storage: None,
                    ingestion: Mutex::new(None),
                    init_error: Some(format!(
                        "application data directory is unavailable: {error}"
                    )),
                    ingestion_error: None,
                };
            }
        };

        let storage = match Storage::open(&path, RetentionPolicy::default()) {
            Ok(storage) => Arc::new(storage),
            Err(error) => {
                return Self {
                    storage: None,
                    ingestion: Mutex::new(None),
                    init_error: Some(format!("event database unavailable: {error}")),
                    ingestion_error: None,
                };
            }
        };

        let source: Arc<dyn EventSource> = Arc::new(IpcEventSource::production());
        let (ingestion, ingestion_error) = match IngestionHandle::start(
            Arc::clone(&storage),
            source,
            IngestionConfig::default(),
        ) {
            Ok(handle) => (Some(handle), None),
            Err(error) => (
                None,
                Some(format!("event ingestion did not start: {error}")),
            ),
        };

        Self {
            storage: Some(storage),
            ingestion: Mutex::new(ingestion),
            init_error: None,
            ingestion_error,
        }
    }

    pub fn storage(&self) -> Option<Arc<Storage>> {
        self.storage.clone()
    }

    pub fn ingestion_session_id(&self) -> Option<String> {
        self.ingestion
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .and_then(|handle| handle.status().session_id)
    }

    pub fn report(&self) -> StorageRuntimeReport {
        let ingestion = self
            .ingestion
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(IngestionHandle::status)
            .unwrap_or_default();
        let storage = match &self.storage {
            Some(storage) => match storage.status() {
                Ok(status) => Some(status),
                Err(error) => {
                    return StorageRuntimeReport {
                        state: "unavailable",
                        init_error: Some(format!("event database status failed: {error}")),
                        ingestion_error: self.ingestion_error.clone(),
                        storage: None,
                        ingestion: ingestion.clone(),
                        last_error: Some(format!("event database status failed: {error}")),
                    };
                }
            },
            None => None,
        };
        let degraded = self.init_error.is_some()
            || self.ingestion_error.is_some()
            || matches!(
                ingestion.state,
                IngestionState::Degraded | IngestionState::Stopped
            );
        let state = if self.storage.is_none() {
            "unavailable"
        } else if degraded {
            "degraded"
        } else {
            "ready"
        };
        let last_error = self
            .init_error
            .clone()
            .or_else(|| self.ingestion_error.clone())
            .or_else(|| ingestion.last_error.clone());
        StorageRuntimeReport {
            state,
            init_error: self.init_error.clone(),
            ingestion_error: self.ingestion_error.clone(),
            storage,
            ingestion,
            last_error,
        }
    }

    pub fn stop(&self) {
        if let Some(handle) = self
            .ingestion
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
        {
            handle.stop();
        }
    }
}
