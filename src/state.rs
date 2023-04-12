use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadRecord {
    pub uploaded: DateTime<Utc>,
    pub file: PathBuf,
    pub downloads: u8,
    pub max_downloads: u8,
}

impl UploadRecord {
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            ..Default::default()
        }
    }
}

impl Default for UploadRecord {
    fn default() -> Self {
        Self {
            uploaded: Utc::now(),
            file: Path::new("").to_owned(),
            downloads: 0,
            max_downloads: 5,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub records: Arc<Mutex<HashMap<String, UploadRecord>>>,
}

impl AppState {
    pub fn new(records: HashMap<String, UploadRecord>) -> Self {
        Self {
            records: Arc::new(Mutex::new(records)),
        }
    }
}
