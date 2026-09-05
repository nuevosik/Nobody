use std::path::PathBuf;

use super::cache;

pub fn resolve_named_icon(name: &str) -> Option<PathBuf> {
    let name = name.trim().trim_end_matches(".desktop").trim_start_matches("file://");
    if name.is_empty() || name.len() > 512 {
        return None;
    }

    if let Some(hit) = cache::cached(name) {
        return hit;
    }

    let found = lookup_named_icon(name);
    cache::store(name, found.clone());
    found
}

pub(crate) fn lookup_named_icon(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') {
        let path = PathBuf::from(name);
        if !matches!(
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase()),
            Some(ref e) if matches!(e.as_str(), "png" | "svg" | "jpg" | "jpeg" | "webp")
        ) {
            return None;
        }
        if path.is_file() {
            return Some(path);
        }
        return None;
    }

    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return None;
    }

    const SIZES: &[&str] =
        &["scalable", "512x512", "256x256", "128x128", "64x64", "48x48", "32x32", "22x22", "16x16"];
    let mut roots = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if let Some(home) = data_home {
        roots.push(home.join("icons"));
    }
    if let Some(dirs) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&dirs) {
            roots.push(dir.join("icons"));
        }
    } else {
        roots.push(PathBuf::from("/usr/share/icons"));
        roots.push(PathBuf::from("/usr/local/share/icons"));
    }
    roots.push(PathBuf::from("/usr/share/pixmaps"));

    for root in &roots {
        for size in SIZES {
            for ext in ["svg", "png"] {
                let path =
                    root.join("hicolor").join(size).join("apps").join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
                let path = root.join(size).join("apps").join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
                let path = root.join(size).join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        for ext in ["svg", "png"] {
            let path = root.join(format!("{name}.{ext}"));
            if path.is_file() {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_icon_names_are_ignored() {
        assert!(resolve_named_icon(&"a".repeat(513)).is_none());
    }

    #[test]
    fn file_uri_traversal_is_rejected() {
        assert!(resolve_named_icon("file:///etc/passwd").is_none());
        assert!(resolve_named_icon("file:////etc/passwd").is_none());
        assert!(resolve_named_icon("file://../../etc/passwd").is_none());
        assert!(resolve_named_icon("file://..\\..\\windows").is_none());
        assert!(resolve_named_icon("file://a/b").is_none());
    }

    #[test]
    fn thematic_lookup_rejects_directories() {
        let base = std::env::temp_dir().join(format!("nobody-icondir-{}", std::process::id()));
        let appdir = base.join("icons/hicolor/48x48/apps");
        std::fs::create_dir_all(&appdir).unwrap();
        let name = "nobody-test-dir-icon-xyz987";
        std::fs::create_dir_all(appdir.join(format!("{name}.png"))).unwrap();
        let old_home = std::env::var_os("XDG_DATA_HOME");
        let old_dirs = std::env::var_os("XDG_DATA_DIRS");
        unsafe {
            std::env::set_var("XDG_DATA_HOME", &base);
            std::env::set_var("XDG_DATA_DIRS", "/nonexistent-nobody-xyz");
        }
        let got = resolve_named_icon(name);
        unsafe {
            match old_home {
                Some(v) => std::env::set_var("XDG_DATA_HOME", v),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match old_dirs {
                Some(v) => std::env::set_var("XDG_DATA_DIRS", v),
                None => std::env::remove_var("XDG_DATA_DIRS"),
            }
        }
        std::fs::remove_dir_all(&base).ok();
        assert!(got.is_none(), "diretório não pode resolver como ícone: {got:?}");
    }
}
