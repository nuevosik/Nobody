use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

pub const ICON_CACHE_LIMIT: usize = 256;

static CACHE: LazyLock<Mutex<HashMap<String, Option<PathBuf>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn cached(name: &str) -> Option<Option<PathBuf>> {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned()
}

pub(crate) fn store(name: &str, found: Option<PathBuf>) {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= ICON_CACHE_LIMIT && !cache.contains_key(name) {
        let to_remove = ICON_CACHE_LIMIT / 4;
        let keys: Vec<String> = cache.keys().take(to_remove).cloned().collect();
        for k in keys {
            cache.remove(&k);
        }
    }
    cache.insert(name.to_string(), found);
}
