use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use crate::application::config::{self, Config};

pub fn config_path(xdg_config_home: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(value) = xdg_config_home.filter(|value| !value.is_empty()) {
        let path = Path::new(value);
        if path.is_absolute() {
            return Some(path.join("nobody/config"));
        }
    }
    let home = home.filter(|value| !value.is_empty())?;
    Some(Path::new(home).join(".config/nobody/config"))
}

pub fn load() -> Config {
    let path = config_path(
        std::env::var_os("XDG_CONFIG_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
    );
    load_path(path.as_deref())
}

pub fn load_path(path: Option<&Path>) -> Config {
    let Some(path) = path else { return Config::default() };
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Config::default(),
        Err(error) => return invalid(path, format!("não foi possível ler ({error})")),
    };
    let mut bytes = Vec::with_capacity(config::MAX_CONFIG_BYTES + 1);
    if let Err(error) =
        file.by_ref().take((config::MAX_CONFIG_BYTES + 1) as u64).read_to_end(&mut bytes)
    {
        return invalid(path, format!("não foi possível ler ({error})"));
    }
    if bytes.len() > config::MAX_CONFIG_BYTES {
        return invalid(path, format!("arquivo maior que {} bytes", config::MAX_CONFIG_BYTES));
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return invalid(path, "UTF-8 inválido".into()),
    };
    match config::parse(&text) {
        Ok(config) => config,
        Err(error) => invalid(path, error),
    }
}

fn invalid(path: &Path, reason: String) -> Config {
    eprintln!("nobody: configuração inválida em {}: {reason}; usando defaults", path.display());
    Config::default()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::Path;

    use super::*;
    use crate::application::config::{Anchor, Config};

    #[test]
    fn prefers_absolute_xdg_and_falls_back_to_home() {
        assert_eq!(
            config_path(Some(OsStr::new("/tmp/config")), Some(OsStr::new("/home/test"))),
            Some(Path::new("/tmp/config/nobody/config").to_path_buf())
        );
        assert_eq!(
            config_path(Some(OsStr::new("relative")), Some(OsStr::new("/home/test"))),
            Some(Path::new("/home/test/.config/nobody/config").to_path_buf())
        );
        assert_eq!(config_path(None, None), None);
    }

    #[test]
    fn missing_file_is_silent_default() {
        let path =
            std::env::temp_dir().join(format!("nobody-config-missing-{}", std::process::id()));
        assert_eq!(load_path(Some(&path)), Config::default());
    }

    #[test]
    fn reads_valid_file_and_rejects_large_or_invalid_files() {
        let dir = std::env::temp_dir().join(format!("nobody-config-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config");
        std::fs::write(&path, "default-timeout=123\nmax-visible=2\nanchor=top-left\n").unwrap();
        assert_eq!(load_path(Some(&path)).default_timeout_ms, 123);
        assert_eq!(load_path(Some(&path)).max_visible, 2);
        assert_eq!(load_path(Some(&path)).anchor, Anchor::TopLeft);
        std::fs::write(&path, vec![b'x'; config::MAX_CONFIG_BYTES + 1]).unwrap();
        assert_eq!(load_path(Some(&path)), Config::default());
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert_eq!(load_path(Some(&path)), Config::default());
        assert_eq!(load_path(Some(&dir)), Config::default());
        std::fs::remove_dir_all(&dir).ok();
    }
}
