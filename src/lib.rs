use chrono::{DateTime, Utc};
use rama::http::{HeaderMap, Method, StatusCode, header};
use std::{collections::HashMap, path::PathBuf, sync::RwLock};

pub mod cli;
pub mod error;
pub mod server;

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
}

impl DataStore {
    pub fn new(max_store_size: u64, store: PathBuf) -> Self {
        Self {
            max_store_size,
            urls: RwLock::new(HashMap::new()),
            store,
        }
    }

    pub fn get(&self, url: &str) -> Option<CacheEntry> {
        let urls = self.urls.read().ok()?;
        urls.get(url).cloned()
    }

    pub fn insert(&self, url: String, entry: CacheEntry) {
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
