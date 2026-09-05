use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::process::Command;

use zbus::zvariant::OwnedValue;

const SPOTIFY_BUS: &str = "org.mpris.MediaPlayer2.spotify";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const ART_URL_KEY: &str = "mpris:artUrl";
const CURL_TIMEOUT_S: &str = "10";
const MAX_BYTES: &str = "5242880";

pub fn is_spotify(app: &str, hints: &HashMap<String, OwnedValue>) -> bool {
    if app.trim().eq_ignore_ascii_case("spotify") {
        return true;
    }
    ["desktop-entry", "desktop_entry"].iter().any(|key| {
        hints
            .get(*key)
            .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
            .is_some_and(|id| id.to_lowercase().contains("spotify"))
    })
}

pub fn spotify_cover(app: &str, hints: &HashMap<String, OwnedValue>) -> Option<PathBuf> {
    if !is_spotify(app, hints) {
        return None;
    }
    let url = mpris_art_url()?;
    if let Some(path) = file_url_to_path(&url) {
        return path.exists().then_some(path);
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        return cached_download(&url);
    }
    None
}

fn mpris_art_url() -> Option<String> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        SPOTIFY_BUS,
        MPRIS_PATH,
        "org.freedesktop.DBus.Properties",
    )
    .ok()?;
    let raw: OwnedValue = proxy.call("Get", &(PLAYER_IFACE, "Metadata")).ok()?;
    art_url_from_value(&raw)
}

fn drill_to_string(v: &zbus::zvariant::Value) -> Option<String> {
    match v {
        zbus::zvariant::Value::Str(s) => Some(s.as_str().to_owned()),
        zbus::zvariant::Value::Value(inner) => drill_to_string(inner),
        _ => None,
    }
}

fn art_url_from_value(root: &OwnedValue) -> Option<String> {
    let v = zbus::zvariant::Value::from(root.try_clone().ok()?);
    let dict_value = match &v {
        zbus::zvariant::Value::Value(inner) => inner.as_ref(),
        v => v,
    };
    let zbus::zvariant::Value::Dict(dict) = dict_value else {
        return None;
    };
    for (k, val) in dict.iter() {
        if matches!(k, zbus::zvariant::Value::Str(s) if s.as_str() == ART_URL_KEY) {
            return drill_to_string(val).filter(|s| !s.is_empty());
        }
    }
    None
}

fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let stripped = url.strip_prefix("file://")?;
    Some(PathBuf::from(stripped))
}

fn covers_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    let dir = base.join("nobody").join("covers");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn cache_path_for(url: &str, ext: &str) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    Some(covers_dir()?.join(format!("{:016x}.{ext}", hasher.finish())))
}

fn cached_download(url: &str) -> Option<PathBuf> {
    if let Some(dir) = covers_dir() {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        let stem = format!("{:016x}", hasher.finish());
        for ext in ["jpg", "jpeg", "png", "webp"] {
            let p = dir.join(format!("{stem}.{ext}"));
            if p.exists() {
                return Some(p);
            }
        }
    }
    let tmp = std::env::temp_dir().join(format!("nobody-cover-{}", std::process::id()));
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            CURL_TIMEOUT_S,
            "--max-filesize",
            MAX_BYTES,
            "-o",
            &tmp.to_string_lossy(),
            url,
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|_| std::fs::read(&tmp).ok())?;
    std::fs::remove_file(&tmp).ok();
    if out.is_empty() {
        return None;
    }
    let ext = image::guess_format(&out).ok()?.extensions_str().first()?;
    let dest = cache_path_for(url, ext)?;
    std::fs::write(&dest, &out).ok()?;
    Some(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints_with(key: &str, val: &str) -> HashMap<String, OwnedValue> {
        use zbus::zvariant::Value;
        HashMap::from([(key.into(), OwnedValue::try_from(Value::from(val)).unwrap())])
    }

    #[test]
    fn matches_spotify_app_name_case_insensitively() {
        assert!(is_spotify("Spotify", &HashMap::new()));
        assert!(is_spotify("spotify", &HashMap::new()));
        assert!(is_spotify("  SPOTIFY  ", &HashMap::new()));
        assert!(!is_spotify("Firefox", &HashMap::new()));
        assert!(!is_spotify("", &HashMap::new()));
    }

    #[test]
    fn matches_spotify_desktop_entry_hint() {
        assert!(is_spotify("App", &hints_with("desktop-entry", "spotify")));
        assert!(is_spotify("App", &hints_with("desktop_entry", "com.spotify.Client")));
        assert!(!is_spotify("App", &hints_with("desktop-entry", "firefox")));
    }

    #[test]
    fn non_spotify_never_queries_mpris() {
        assert!(spotify_cover("Firefox", &HashMap::new()).is_none());
    }

    #[test]
    fn extracts_art_url_from_bus_shaped_value() {
        use zbus::zvariant::{Dict, Signature, Value};
        let mut dict = Dict::new(&Signature::Str, &Signature::Variant);
        dict.append(
            Value::Str("mpris:artUrl".into()),
            Value::Value(Box::new(Value::Str("https://i.scdn.co/image/abc".into()))),
        )
        .unwrap();
        let root = OwnedValue::try_from(Value::Value(Box::new(Value::Dict(dict)))).unwrap();
        assert_eq!(art_url_from_value(&root).as_deref(), Some("https://i.scdn.co/image/abc"));
    }

    #[test]
    fn rejects_empty_and_missing_art_url() {
        use zbus::zvariant::{Dict, Signature, Value};
        let root = OwnedValue::try_from(Value::Value(Box::new(Value::Dict(Dict::new(
            &Signature::Str,
            &Signature::Variant,
        )))))
        .unwrap();
        assert!(art_url_from_value(&root).is_none());

        let mut dict = Dict::new(&Signature::Str, &Signature::Variant);
        dict.append(
            Value::Str("mpris:artUrl".into()),
            Value::Value(Box::new(Value::Str("".into()))),
        )
        .unwrap();
        let root = OwnedValue::try_from(Value::Value(Box::new(Value::Dict(dict)))).unwrap();
        assert!(art_url_from_value(&root).is_none());
    }

    #[test]
    fn cache_path_is_stable_and_url_specific() {
        let a = cache_path_for("https://i.scdn.co/image/abc", "jpg").unwrap();
        let b = cache_path_for("https://i.scdn.co/image/abc", "jpg").unwrap();
        let c = cache_path_for("https://i.scdn.co/image/def", "jpg").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.extension().unwrap(), "jpg");
    }

    #[test]
    fn missing_local_file_is_none() {
        assert!(
            file_url_to_path("file:///nonexistent-nobody-xyz.png").is_some_and(|p| !p.exists())
        );
    }
}
