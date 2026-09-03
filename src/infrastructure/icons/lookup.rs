//! Infra/icons — lookup de nomes temáticos e caminhos.
use std::path::PathBuf;

use super::cache;

/// Resolve um nome de ícone ou caminho para um PNG/SVG no disco.
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
                if path.is_file() {
                    return Some(path);
                }
                let path = root.join(size).join("apps").join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
                // tema Adwaita/ Papirus sem subdir apps
                let path = root.join(size).join(format!("{name}.{ext}"));
                if path.is_file() {
                    return Some(path);
                }
            }
        }
        for ext in ["png", "svg"] {
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
