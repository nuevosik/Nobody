//! Resolução de ícones de notificação, portado da rot (`providers/notices.rs`
//! + `providers/tray.rs`).
//!
//! Acha um PNG/SVG no disco a partir de nome, hint ou desktop entry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use zbus::zvariant::OwnedValue;

const ICON_CACHE_LIMIT: usize = 256;

pub fn resolve_notice_icon(
    app_icon: &str,
    app: &str,
    hints: &HashMap<String, OwnedValue>,
) -> Option<PathBuf> {
    for key in ["image-path", "image_path"] {
        if let Some(path) = hint_string(hints, key).and_then(|p| resolve_named_icon(&p)) {
            return Some(path);
        }
    }
    if let Some(path) = resolve_named_icon(app_icon) {
        return Some(path);
    }
    for key in ["desktop-entry", "desktop_entry"] {
        if let Some(id) = hint_string(hints, key) {
            if let Some(path) = icon_from_desktop(&id) {
                return Some(path);
            }
            if let Some(path) = resolve_named_icon(&id) {
                return Some(path);
            }
        }
    }
    if let Some(path) = resolve_named_icon(app) {
        return Some(path);
    }
    None
}

fn hint_string(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    hints
        .get(key)
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .filter(|s| !s.is_empty())
}

fn icon_from_desktop(id: &str) -> Option<PathBuf> {
    let id = id.trim().trim_end_matches(".desktop");
    if id.is_empty() || id.len() > 255 || id.contains('/') || id.contains("..") {
        return None;
    }
    let mut files = Vec::new();
    // XDG_DATA_DIRS + XDG_DATA_HOME
    if let Some(dirs) = std::env::var_os("XDG_DATA_DIRS") {
        for dir in std::env::split_paths(&dirs) {
            files.push(dir.join(format!("applications/{id}.desktop")));
        }
    } else {
        files.push(PathBuf::from(format!("/usr/share/applications/{id}.desktop")));
        files.push(PathBuf::from("/usr/local/share/applications/{id}.desktop"));
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if let Some(home) = data_home {
        files.push(home.join(format!("applications/{id}.desktop")));
    }
    for file in files {
        if let Some(icon) = desktop_icon_key(&file)
            && let Some(path) = resolve_named_icon(&icon)
        {
            return Some(path);
        }
    }
    None
}

fn desktop_icon_key(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    // limita a 16KB para não ler desktop malicioso gigante
    if text.len() > 16 * 1024 {
        return None;
    }
    text.lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                return None;
            }
            trimmed.strip_prefix("Icon=")
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Resolve um nome de ícone ou caminho para um PNG/SVG no disco.
pub fn resolve_named_icon(name: &str) -> Option<PathBuf> {
    let name = name.trim().trim_end_matches(".desktop").trim_start_matches("file://");
    if name.is_empty() || name.len() > 512 {
        return None;
    }

    static CACHE: LazyLock<Mutex<HashMap<String, Option<PathBuf>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if let Some(hit) = CACHE.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned() {
        return hit;
    }

    let found = lookup_named_icon(name);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= ICON_CACHE_LIMIT && !cache.contains_key(name) {
        // evita clear() que joga fora tudo: remove 1/4 mais antigo (HashMap ordem aleatória, mas limita)
        let to_remove = ICON_CACHE_LIMIT / 4;
        let keys: Vec<String> = cache.keys().take(to_remove).cloned().collect();
        for k in keys {
            cache.remove(&k);
        }
    }
    cache.insert(name.to_string(), found.clone());
    found
}

fn lookup_named_icon(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') {
        let path = PathBuf::from(name);
        // só aceita imagem explícita; rejeita /etc/passwd e similares
        if !matches!(
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase()),
            Some(ref e) if matches!(e.as_str(), "png" | "svg" | "jpg" | "jpeg" | "webp")
        ) {
            return None;
        }
        // valida existência e que é arquivo regular
        if path.is_file() {
            return Some(path);
        }
        return None;
    }

    // rejeita traversal em nomes temáticos (join nunca deve escapar root)
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return None;
    }

    // Ordem: escalável primeiro, depois maiores p/ menores
    const SIZES: &[&str] =
        &["scalable", "256x256", "128x128", "64x64", "48x48", "32x32", "22x22", "16x16"];
    let mut roots = Vec::new();
    // XDG_DATA_HOME/icons
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if let Some(home) = data_home {
        roots.push(home.join("icons"));
    }
    // XDG_DATA_DIRS
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
            for ext in ["png", "svg"] {
                let path =
                    root.join("hicolor").join(size).join("apps").join(format!("{name}.{ext}"));
                if path.exists() {
                    return Some(path);
                }
                let path = root.join(size).join("apps").join(format!("{name}.{ext}"));
                if path.exists() {
                    return Some(path);
                }
                // tema Adwaita/ Papirus sem subdir apps
                let path = root.join(size).join(format!("{name}.{ext}"));
                if path.exists() {
                    return Some(path);
                }
            }
        }
        for ext in ["png", "svg"] {
            let path = root.join(format!("{name}.{ext}"));
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn desktop_entry_lookup_rejects_paths() {
        assert!(icon_from_desktop("../../etc/passwd").is_none());
        assert!(icon_from_desktop("nested/entry").is_none());
    }

    #[test]
    fn oversized_icon_names_are_ignored() {
        assert!(resolve_named_icon(&"a".repeat(513)).is_none());
    }

    #[test]
    fn file_uri_traversal_is_rejected() {
        // `file://` apenas descasca o esquema; traversal continua bloqueado:
        // - sem extensão de imagem -> None (mesmo que /etc/passwd exista)
        // - `..` com `/` cai na rejeição temática
        assert!(resolve_named_icon("file:///etc/passwd").is_none());
        assert!(resolve_named_icon("file:////etc/passwd").is_none());
        assert!(resolve_named_icon("file://../../etc/passwd").is_none());
        assert!(resolve_named_icon("file://..\\..\\windows").is_none());
        // sanity: nome temático com `/` continua rejeitado mesmo via file://
        assert!(resolve_named_icon("file://a/b").is_none());
    }

    #[test]
    fn desktop_icon_uses_entry_section_only() {
        // Icon sob [Desktop Action ...] NÃO pode vazar como ícone principal.
        let dir = std::env::temp_dir().join(format!("nobody-sec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sec.desktop");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "[Desktop Action Open]").unwrap();
        writeln!(f, "Name=Open").unwrap();
        writeln!(f, "Icon=wrong-icon").unwrap();
        writeln!(f, "[Desktop Entry]").unwrap();
        writeln!(f, "Name=App").unwrap();
        writeln!(f, "Icon=right-icon").unwrap();
        drop(f);
        let got = desktop_icon_key(&file).unwrap_or_default();
        std::fs::remove_file(&file).ok();
        std::fs::remove_dir(&dir).ok();
        assert_eq!(got, "right-icon");
    }

    #[test]
    fn desktop_huge_file_is_rejected() {
        // >16KB deve retornar None (regressão; fix evita ler tudo antes do limite).
        let dir = std::env::temp_dir().join(format!("nobody-huge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("huge.desktop");
        let mut f = std::fs::File::create(&file).unwrap();
        writeln!(f, "[Desktop Entry]").unwrap();
        // ~32KB de preenchimento antes do Icon
        let pad = "A".repeat(32 * 1024);
        writeln!(f, "Comment={pad}").unwrap();
        writeln!(f, "Icon=late-icon").unwrap();
        drop(f);
        let got = desktop_icon_key(&file);
        std::fs::remove_file(&file).ok();
        std::fs::remove_dir(&dir).ok();
        assert!(got.is_none());
    }

    #[test]
    fn thematic_lookup_rejects_directories() {
        // `exists()` aceita diretório; `is_file()` não. Cria
        // $XDG_DATA_HOME/icons/hicolor/48x48/apps/<name>.png como DIRETÓRIO.
        let base = std::env::temp_dir().join(format!("nobody-icondir-{}", std::process::id()));
        let appdir = base.join("icons/hicolor/48x48/apps");
        std::fs::create_dir_all(&appdir).unwrap();
        let name = "nobody-test-dir-icon-xyz987";
        std::fs::create_dir_all(appdir.join(format!("{name}.png"))).unwrap();
        // isola lookup para o temp (salva/restaura env; nome único evita colisão)
        let old_home = std::env::var_os("XDG_DATA_HOME");
        let old_dirs = std::env::var_os("XDG_DATA_DIRS");
        // limpa cache negativo anterior? nome único => sem colisão de cache.
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
