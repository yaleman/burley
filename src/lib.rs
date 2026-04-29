use chrono::{DateTime, Utc};
use concread::hashmap::HashMap;
use std::path::PathBuf;

pub mod cli;
pub mod error;
pub mod server;

#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub content: Vec<u8>,
    pub timestamp: DateTime<Utc>,
    pub file_ref: String,
}

pub struct DataStore {
    pub max_store_size: u64,
    pub urls: HashMap<String, CacheEntry>,
    pub store: PathBuf,
}

impl DataStore {
    pub fn new(max_store_size: u64, store: PathBuf) -> Self {
        Self {
            max_store_size,
            urls: HashMap::new(),
            store,
        }
    }
}
