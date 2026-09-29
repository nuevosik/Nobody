use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use zbus::zvariant::OwnedValue;

use crate::infrastructure::cache_file;

const SPOTIFY_BUS: &str = "org.mpris.MediaPlayer2.spotify";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_IFACE: &str = "org.mpris.MediaPlayer2.Player";
const ART_URL_KEY: &str = "mpris:artUrl";
const MPRIS_TIMEOUT: Duration = Duration::from_secs(2);
const CURL_TIMEOUT_S: &str = "10";
const MAX_BYTES: usize = 5 * 1024 * 1024;

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
        return path.is_file().then_some(path);
    }
    if allowed_art_host(&url) {
        return cached_download(&url);
    }
    None
}

fn allowed_art_host(url: &str) -> bool {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"));
    let Some(rest) = rest else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    host.ends_with(".scdn.co") || host.ends_with(".spotifycdn.com")
}

fn mpris_art_url() -> Option<String> {
    let conn = zbus::blocking::connection::Builder::session()
        .ok()?
        .method_timeout(MPRIS_TIMEOUT)
        .build()
        .ok()?;
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

fn cache_path_for(url: &str, ext: &str) -> Option<PathBuf> {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    Some(cache_file::cache_dir("covers")?.join(format!("{:016x}.{ext}", hasher.finish())))
}

fn cached_download(url: &str) -> Option<PathBuf> {
    if let Some(dir) = cache_file::cache_dir("covers") {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        let stem = format!("{:016x}", hasher.finish());
        for ext in ["jpg", "jpeg", "png", "webp"] {
            let p = dir.join(format!("{stem}.{ext}"));
            if p.symlink_metadata().ok().is_some_and(|meta| meta.file_type().is_file()) {
                return Some(p);
            }
        }
    }
    let max_bytes = MAX_BYTES.to_string();
    let mut child = Command::new("curl")
        .args(["-fsSL", "--max-time", CURL_TIMEOUT_S, "--max-filesize", &max_bytes, url])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = Vec::new();
    let read = child.stdout.take()?.take(MAX_BYTES as u64 + 1).read_to_end(&mut out);
    if read.is_err() || out.len() > MAX_BYTES {
        child.kill().ok();
    }
    let status = child.wait().ok()?;
    read.ok()?;
    if !status.success() || out.len() > MAX_BYTES {
        return None;
    }
    if out.is_empty() {
        return None;
    }
    let ext = image::guess_format(&out).ok()?.extensions_str().first()?;
    let dest = cache_path_for(url, ext)?;
    cache_file::write_atomic(&dest, &out)?;
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

    #[test]
    fn art_host_allowlist_blocks_ssrf() {
        assert!(allowed_art_host("https://i.scdn.co/image/abc"));
        assert!(allowed_art_host("http://mosaic.scdn.co/640/abc"));
        assert!(allowed_art_host("https://image-cdn-fa.spotifycdn.com/image/abc"));
        assert!(allowed_art_host("https://i.scdn.co:443/image/abc"));
        assert!(!allowed_art_host("http://127.0.0.1:8080/x"));
        assert!(!allowed_art_host("http://169.254.169.254/latest/meta-data/"));
        assert!(!allowed_art_host("https://evil.example/spotify"));
        assert!(!allowed_art_host("https://evilscdn.co/x"));
        assert!(!allowed_art_host("file:///etc/passwd"));
        assert!(!allowed_art_host("ftp://i.scdn.co/x"));
    }
}
