//! Application data directory resolution.
//!
//! The database must live under the per-user local application data directory;
//! it is never placed in the repository or a temporary directory. The caller
//! may pass the Tauri-provided local data directory explicitly.

use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "SentinelLite";
pub const DATABASE_FILE_NAME: &str = "events.db";

/// `%LOCALAPPDATA%\SentinelLite\events.db`.
pub fn default_database_path() -> Result<PathBuf, String> {
    let base = std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "LOCALAPPDATA is not available".to_owned())?;
    Ok(PathBuf::from(base)
        .join(APP_DIR_NAME)
        .join(DATABASE_FILE_NAME))
}

/// Use a Tauri-resolved local data directory as the database parent.
pub fn database_path_in(app_local_data_dir: &Path) -> PathBuf {
    app_local_data_dir
        .join(APP_DIR_NAME)
        .join(DATABASE_FILE_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_path_is_under_the_given_app_data_directory() {
        let path = database_path_in(Path::new(
            r"C:\Users\test\AppData\Local\app.sentinellite.desktop",
        ));
        assert!(path.ends_with(Path::new("SentinelLite").join("events.db")));
    }
}
