use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::cache;

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    pub fn can_be_downloaded(&self) -> bool {
        let dur_since_upload = Utc::now().signed_duration_since(self.uploaded);

        dur_since_upload < Duration::days(3) && self.downloads < self.max_downloads
    }

    pub fn downloads_remaining(&self) -> u8 {
        self.max_downloads - self.downloads
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

#[async_trait]
pub trait AsyncRemoveRecord {
    async fn remove_record(&mut self, id: &String) -> Result<(), std::io::Error>;
}

#[async_trait]
impl AsyncRemoveRecord for AppState {
    async fn remove_record(&mut self, id: &String) -> Result<(), std::io::Error> {
        let mut records = self.records.lock().await;

        if let Some(record) = records.get_mut(id) {
            tokio::fs::remove_file(&record.file).await?;
            records.remove(id);
            cache::write_to_cache(&records).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl AsyncRemoveRecord for HashMap<String, UploadRecord> {
    async fn remove_record(&mut self, id: &String) -> Result<(), std::io::Error> {
        if let Some(record) = self.get_mut(id) {
            tokio::fs::remove_file(&record.file).await?;
            self.remove(id);
            cache::write_to_cache(&self).await?;
        }

        Ok(())
    }
}
