use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn cache_dir_path(
    xdg_cache_home: Option<&OsStr>,
    home: Option<&OsStr>,
    kind: &str,
) -> Option<PathBuf> {
    let base = match xdg_cache_home.filter(|value| !value.is_empty()) {
        Some(value) if Path::new(value).is_absolute() => PathBuf::from(value),
        _ => Path::new(home.filter(|value| !value.is_empty())?).join(".cache"),
    };
    Some(base.join("nobody").join(kind))
}

pub(crate) fn cache_dir(kind: &str) -> Option<PathBuf> {
    let dir = cache_dir_path(
        std::env::var_os("XDG_CACHE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        kind,
    )?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

pub(crate) fn write_atomic(dest: &Path, bytes: &[u8]) -> Option<()> {
    for _ in 0..8 {
        let tmp = dest.with_extension(format!(
            "tmp-{}-{}",
            std::process::id(),
            NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let Ok(mut file) = File::create_new(&tmp) else {
            continue;
        };
        let written = file.write_all(bytes).is_ok();
        drop(file);
        if written && std::fs::rename(&tmp, dest).is_ok() {
            return Some(());
        }
        std::fs::remove_file(&tmp).ok();
        return None;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn write_atomic_replaces_symlink_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let dir = std::env::temp_dir().join(format!(
            "nobody-atomic-test-{}-{}",
            std::process::id(),
            NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let target = dir.join("target");
        let dest = dir.join("cover.jpg");
        std::fs::write(&target, b"keep").unwrap();
        symlink(&target, &dest).unwrap();

        assert_eq!(write_atomic(&dest, b"image"), Some(()));
        assert_eq!(std::fs::read(&target).unwrap(), b"keep");
        assert_eq!(std::fs::read(&dest).unwrap(), b"image");
        assert!(std::fs::symlink_metadata(&dest).unwrap().file_type().is_file());

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cache_dir_path_ignores_empty_and_relative_xdg() {
        let home = OsStr::new("/home/test");
        assert_eq!(
            cache_dir_path(Some(OsStr::new("/tmp/cache")), Some(home), "inline"),
            Some(PathBuf::from("/tmp/cache/nobody/inline"))
        );
        assert_eq!(
            cache_dir_path(Some(OsStr::new("/tmp/cache")), None, "covers"),
            Some(PathBuf::from("/tmp/cache/nobody/covers")),
            "an absolute XDG wins even without HOME"
        );
        for rejected in ["", "relative", "./cache"] {
            assert_eq!(
                cache_dir_path(Some(OsStr::new(rejected)), Some(home), "inline"),
                Some(PathBuf::from("/home/test/.cache/nobody/inline")),
                "{rejected:?} must not be used as a base"
            );
        }
        assert_eq!(
            cache_dir_path(None, Some(home), "inline"),
            Some(PathBuf::from("/home/test/.cache/nobody/inline"))
        );
        assert_eq!(cache_dir_path(Some(OsStr::new("")), None, "inline"), None);
        assert_eq!(cache_dir_path(None, None, "inline"), None);
    }
}
