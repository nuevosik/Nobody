//! Infra/icons — desktop entries (.desktop).
use std::path::{Path, PathBuf};

use super::lookup::resolve_named_icon;

pub(crate) fn icon_from_desktop(id: &str) -> Option<PathBuf> {
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

pub(crate) fn desktop_icon_key(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    // limita a 16KB para não ler desktop malicioso gigante
    if text.len() > 16 * 1024 {
        return None;
    }
    let mut in_entry = false;
    text.lines()
        .find_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                return None;
            }
            // só a seção [Desktop Entry] vale; Icon sob [Desktop Action ...]
            // ou outra seção não pode vazar como ícone principal.
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                in_entry = trimmed == "[Desktop Entry]";
                return None;
            }
            if !in_entry {
                return None;
            }
            trimmed.strip_prefix("Icon=")
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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
}
