use chrono::{DateTime, Utc};
use rama::http::{HeaderMap, Method, StatusCode, header};
use std::{collections::HashMap, path::PathBuf, sync::RwLock};

use crate::stats::StatsMeters;

pub mod cli;
pub mod error;
pub mod logging;
pub mod server;
pub mod stats;

#[derive(Clone, Debug)]
pub struct CacheEntry {
    pub content: Vec<u8>,
    pub headers: HeaderMap,
    pub status: StatusCode,
    pub timestamp: DateTime<Utc>,
}

pub struct DataStore {
    pub max_store_size: u64,
    pub urls: RwLock<HashMap<String, CacheEntry>>,
    pub store: PathBuf,
    pub metrics: Option<StatsMeters>,
}

impl DataStore {
    pub fn new(max_store_size: u64, store: PathBuf) -> Self {
        Self {
            max_store_size,
            urls: RwLock::new(HashMap::new()),
            store,
            metrics: None,
        }
    }
    pub fn with_metrics(self, metrics_meter: opentelemetry_sdk::metrics::SdkMeterProvider) -> Self {
        let metrics = Some(crate::stats::init_meters(&metrics_meter));
        Self { metrics, ..self }
    }

    pub fn get(&self, url: &str) -> Option<CacheEntry> {
        let urls = self.urls.read().ok()?;
        urls.get(url).cloned()
    }

    pub fn insert(&self, url: String, entry: CacheEntry) {
        self.metrics.as_ref().map(|metrics| {
            let urls = self.urls.read().ok()?;
            let total_size: u64 = urls.values().map(|entry| entry.content.len() as u64).sum();
            metrics.cache_size.record(total_size, &[]);
            Some(())
        });

        if let Ok(mut urls) = self.urls.write() {
            urls.insert(url, entry);
        }
    }
}

pub fn is_cacheable_response(
    method: &Method,
    status: StatusCode,
    headers: &HeaderMap,
    body_len: usize,
    max_body_len: usize,
) -> bool {
    if method != Method::GET {
        return false;
    }

    if status != StatusCode::OK {
        return false;
    }

    if body_len > max_body_len {
        return false;
    }

    if headers.contains_key(header::SET_COOKIE) {
        return false;
    }

    !headers
        .get_all(header::CACHE_CONTROL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|directive| directive.trim().eq_ignore_ascii_case("no-store"))
}

#[cfg(test)]
mod tests {
    use super::is_cacheable_response;
    use rama::http::{HeaderMap, Method, StatusCode, header};

    #[test]
    fn get_200_without_forbidden_headers_is_cacheable() {
        let headers = HeaderMap::new();

        assert!(is_cacheable_response(
            &Method::GET,
            StatusCode::OK,
            &headers,
            128,
            1024
        ));
    }

    #[test]
    fn non_ok_status_is_not_cacheable() {
        let headers = HeaderMap::new();
        assert!(!is_cacheable_response(
            &Method::GET,
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            &headers,
            128,
            1024
        ));
    }
    #[test]
    fn massive_body_is_not_cacheable() {
        let headers = HeaderMap::new();
        assert!(!is_cacheable_response(
            &Method::GET,
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            &headers,
            1024,
            128,
        ));
    }

    #[test]
    fn non_get_response_is_not_cacheable() {
        let headers = HeaderMap::new();

        assert!(!is_cacheable_response(
            &Method::POST,
            StatusCode::OK,
            &headers,
            128,
            1024
        ));
    }

    #[test]
    fn set_cookie_response_is_not_cacheable() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::SET_COOKIE,
            "session=abc".parse().expect("valid header"),
        );

        assert!(!is_cacheable_response(
            &Method::GET,
            StatusCode::OK,
            &headers,
            128,
            1024
        ));
    }

    #[test]
    fn no_store_response_is_not_cacheable() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CACHE_CONTROL,
            "private, no-store".parse().expect("valid header"),
        );

        assert!(!is_cacheable_response(
            &Method::GET,
            StatusCode::OK,
            &headers,
            128,
            1024
        ));
    }
}
