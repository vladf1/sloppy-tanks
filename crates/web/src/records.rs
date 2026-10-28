//! Personal bests natively: the page's `localStorage` keys and values in one JSON
//! object, in the file the desktop shell names ([`set_records_file`]). Without one,
//! or when it cannot be written, records last only for the session, like private
//! browsing.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use sloppy_core::sim::round_recap::{RecordStorage, StorageUnavailable};

static RECORDS_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Keep personal bests in `path` from now on; set once at startup.
pub fn set_records_file(path: PathBuf) {
    let _ = RECORDS_FILE.set(path);
}

/// The records file's items, read when made and written whole on every change.
pub struct FileStorage {
    path: Option<PathBuf>,
    items: BTreeMap<String, String>,
}

impl FileStorage {
    pub fn local() -> Self {
        Self::at(RECORDS_FILE.get().cloned())
    }

    fn at(path: Option<PathBuf>) -> Self {
        let items = path
            .as_ref()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { path, items }
    }
}

impl RecordStorage for FileStorage {
    fn get_item(&self, key: &str) -> Result<Option<String>, StorageUnavailable> {
        self.path.as_ref().ok_or(StorageUnavailable)?;
        Ok(self.items.get(key).cloned())
    }

    fn set_item(&mut self, key: &str, value: &str) -> Result<(), StorageUnavailable> {
        let path = self.path.as_ref().ok_or(StorageUnavailable)?;
        self.items.insert(key.to_owned(), value.to_owned());
        let text = serde_json::to_string_pretty(&self.items).map_err(|_| StorageUnavailable)?;
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory).map_err(|_| StorageUnavailable)?;
        }
        std::fs::write(path, text).map_err(|_| StorageUnavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_survive_a_new_storage_on_the_same_file() {
        let path = std::env::temp_dir().join(format!("sloppy-records-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut storage = FileStorage::at(Some(path.clone()));
        assert_eq!(storage.get_item("best"), Ok(None));
        assert_eq!(storage.set_item("best", "12"), Ok(()));
        assert_eq!(
            FileStorage::at(Some(path.clone())).get_item("best"),
            Ok(Some("12".into()))
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn without_a_file_records_are_unavailable() {
        let mut storage = FileStorage::at(None);
        assert_eq!(storage.set_item("best", "1"), Err(StorageUnavailable));
    }
}
