use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn cache_dir(kind: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    let dir = base.join("nobody").join(kind);
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
}
