use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use zbus::zvariant::{OwnedValue, Value};

use crate::infrastructure::cache_file;

pub(crate) const MAX_DIMENSION: u32 = 2048;
pub(crate) const INLINE_PX: u32 = 64;
pub(crate) const MAX_FILES: usize = 128;
pub(crate) const MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InlineImage {
    pub width: u32,
    pub height: u32,
    pub rowstride: usize,
    pub channels: u8,
    pub data: Vec<u8>,
}

pub(crate) fn parse(hint: &OwnedValue) -> Option<InlineImage> {
    let value: &Value = hint;
    let Value::Structure(structure) = value else {
        return None;
    };
    let fields = structure.fields();
    if fields.len() != 7 {
        return None;
    }
    let Value::Array(array) = &fields[6] else {
        return None;
    };
    let data: Vec<u8> = Vec::try_from(array.try_clone().ok()?).ok()?;
    validate(
        fields[0].downcast_ref().ok()?,
        fields[1].downcast_ref().ok()?,
        fields[2].downcast_ref().ok()?,
        fields[3].downcast_ref().ok()?,
        fields[4].downcast_ref().ok()?,
        fields[5].downcast_ref().ok()?,
        &data,
    )
}

pub(crate) fn validate(
    width: i32,
    height: i32,
    rowstride: i32,
    has_alpha: bool,
    bits_per_sample: i32,
    channels: i32,
    data: &[u8],
) -> Option<InlineImage> {
    if bits_per_sample != 8 {
        return None;
    }
    let channels = match (channels, has_alpha) {
        (3, false) => 3usize,
        (4, true) => 4usize,
        _ => return None,
    };
    if !(1..=MAX_DIMENSION as i32).contains(&width) || !(1..=MAX_DIMENSION as i32).contains(&height)
    {
        return None;
    }
    if rowstride <= 0 {
        return None;
    }
    let (width, height, rowstride) = (width as usize, height as usize, rowstride as usize);
    let min_stride = width.checked_mul(channels)?;
    if rowstride < min_stride {
        return None;
    }
    let needed = rowstride.checked_mul(height)?;
    if data.len() < needed {
        return None;
    }
    Some(InlineImage {
        width: width as u32,
        height: height as u32,
        rowstride,
        channels: channels as u8,
        data: data.to_vec(),
    })
}

pub(crate) fn to_rgba(image: &InlineImage) -> Option<Vec<u8>> {
    let width = image.width as usize;
    let height = image.height as usize;
    let channels = image.channels as usize;
    let mut out = Vec::with_capacity(width.checked_mul(height)?.checked_mul(4)?);
    for row in 0..height {
        let start = row.checked_mul(image.rowstride)?;
        let end = start.checked_add(width.checked_mul(channels)?)?;
        let line = image.data.get(start..end)?;
        for pixel in line.chunks_exact(channels) {
            out.push(pixel[0]);
            out.push(pixel[1]);
            out.push(pixel[2]);
            out.push(if channels == 4 { pixel[3] } else { 0xff });
        }
    }
    Some(out)
}

fn cache_key(image: &InlineImage) -> u64 {
    let mut hasher = DefaultHasher::new();
    image.width.hash(&mut hasher);
    image.height.hash(&mut hasher);
    image.rowstride.hash(&mut hasher);
    image.channels.hash(&mut hasher);
    image.data.hash(&mut hasher);
    hasher.finish()
}

fn cache_path(image: &InlineImage) -> Option<PathBuf> {
    Some(cache_file::cache_dir("inline")?.join(format!("{:016x}.png", cache_key(image))))
}

pub(crate) fn inline_icon(hints: &HashMap<String, OwnedValue>) -> Option<PathBuf> {
    let hint = hints.get("image-data").or_else(|| hints.get("image_data"))?;
    let image = parse(hint)?;
    let rgba = to_rgba(&image)?;
    let buffer = image::RgbaImage::from_raw(image.width, image.height, rgba)?;
    let small = if image.width > INLINE_PX || image.height > INLINE_PX {
        image::imageops::resize(
            &buffer,
            INLINE_PX,
            INLINE_PX,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        buffer
    };
    let dest = cache_path(&image)?;
    if dest.symlink_metadata().ok().is_some_and(|meta| meta.file_type().is_file()) {
        return Some(dest);
    }
    let mut png = Vec::new();
    small.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).ok()?;
    cache_file::write_atomic(&dest, &png)?;
    evict();
    Some(dest)
}

// ponytail: rescans the cache dir on every new payload; fine at MAX_FILES, index it if the cap grows
fn evict() {
    if let Some(dir) = cache_file::cache_dir("inline") {
        evict_in(&dir);
    }
}

fn evict_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64, std::time::SystemTime)> = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        total += meta.len();
        files.push((entry.path(), meta.len(), meta.modified().unwrap_or(std::time::UNIX_EPOCH)));
    }
    if files.len() <= MAX_FILES && total <= MAX_BYTES {
        return;
    }
    files.sort_by_key(|(_, _, modified)| *modified);
    let mut count = files.len();
    for (path, len, _) in files {
        if count <= MAX_FILES && total <= MAX_BYTES {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            count -= 1;
            total = total.saturating_sub(len);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(width: u32, height: u32) -> Vec<u8> {
        (0..width * height * 4).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn rowstride_padding_does_not_skew_rows() {
        let width = 3usize;
        let height = 2usize;
        let channels = 4usize;
        let rowstride = width * channels + 8;
        let mut data = vec![0u8; rowstride * height];
        for row in 0..height {
            for column in 0..width {
                let at = row * rowstride + column * channels;
                let value = (row * 10 + column) as u8;
                data[at] = value;
                data[at + 1] = value;
                data[at + 2] = value;
                data[at + 3] = 255;
            }
        }
        let image = validate(width as i32, height as i32, rowstride as i32, true, 8, 4, &data)
            .expect("valid image");
        let rgba = to_rgba(&image).expect("rgba");
        assert_eq!(rgba.len(), width * height * 4);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 255]);
        assert_eq!(&rgba[4..8], &[1, 1, 1, 255]);
        assert_eq!(&rgba[12..16], &[10, 10, 10, 255], "second row must start at rowstride");
        assert_eq!(&rgba[16..20], &[11, 11, 11, 255]);
    }

    #[test]
    fn rgb_without_alpha_gets_opaque_alpha() {
        let data = vec![10, 20, 30, 40, 50, 60];
        let image = validate(2, 1, 6, false, 8, 3, &data).expect("valid image");
        let rgba = to_rgba(&image).expect("rgba");
        assert_eq!(rgba, vec![10, 20, 30, 255, 40, 50, 60, 255]);
    }

    #[test]
    fn rejects_malformed_headers_and_truncated_data() {
        let data = vec![0u8; 64];
        assert_eq!(validate(2, 2, 8, true, 16, 4, &data), None, "bits_per_sample");
        assert_eq!(validate(2, 2, 8, true, 8, 3, &data), None, "channels vs has_alpha");
        assert_eq!(validate(2, 2, 8, false, 8, 4, &data), None, "channels vs has_alpha");
        assert_eq!(validate(2, 2, 7, true, 8, 4, &data), None, "rowstride below width*channels");
        assert_eq!(validate(2, 2, 0, true, 8, 4, &data), None, "zero rowstride");
        assert_eq!(validate(2, 2, -8, true, 8, 4, &data), None, "negative rowstride");
        assert_eq!(validate(2, 2, 8, true, 8, 4, &[0u8; 15]), None, "truncated data");
        assert_eq!(validate(0, 2, 8, true, 8, 4, &data), None, "zero width");
        assert_eq!(validate(2, 0, 8, true, 8, 4, &data), None, "zero height");
    }

    #[test]
    fn rejects_oversized_dimensions_and_extreme_values_without_panicking() {
        let data = vec![0u8; 16];
        for (width, height) in [
            (MAX_DIMENSION as i32 + 1, 1),
            (1, MAX_DIMENSION as i32 + 1),
            (i32::MAX, i32::MAX),
            (i32::MIN, i32::MIN),
        ] {
            assert_eq!(validate(width, height, 4, true, 8, 4, &data), None, "{width}x{height}");
        }
        assert_eq!(validate(1, 1, i32::MAX, true, 8, 4, &data), None, "rowstride overflow");
    }

    #[test]
    fn parse_reads_the_wire_structure() {
        use zbus::zvariant::{Array, Str, Structure, Value};

        let data = vec![1u8, 2, 3, 4];
        let structure =
            Structure::from((1i32, 1i32, 4i32, true, 8i32, 4i32, Array::from(&[1u8, 2, 3, 4][..])));
        let hint = OwnedValue::try_from(Value::Structure(structure)).expect("owned value");
        let image = parse(&hint).expect("parses");
        assert_eq!(image.width, 1);
        assert_eq!(image.height, 1);
        assert_eq!(image.rowstride, 4);
        assert_eq!(image.channels, 4);
        assert_eq!(image.data, data);

        let not_a_struct = OwnedValue::try_from(Value::Str(Str::from("nope"))).expect("owned");
        assert_eq!(parse(&not_a_struct), None);
    }

    #[test]
    fn rejects_an_array_that_is_not_a_byte_array() {
        use zbus::zvariant::{Array, Structure, Value};

        let ints: Vec<Value> = (0..16).map(Value::I32).collect();
        let structure =
            Structure::from((4i32, 4i32, 16i32, true, 8i32, 4i32, Array::from(&ints[..])));
        let hint = OwnedValue::try_from(Value::Structure(structure)).expect("owned value");
        assert_eq!(parse(&hint), None, "the spec says ay, an array of i32 must not pass");
    }

    #[test]
    fn identical_payloads_share_one_cache_key() {
        let image = validate(2, 2, 8, true, 8, 4, &pixels(2, 2)).expect("valid image");
        assert_eq!(cache_key(&image), cache_key(&image));
        let other = validate(2, 2, 8, true, 8, 4, &pixels(2, 2)).expect("valid image");
        assert_eq!(cache_key(&image), cache_key(&other), "same bytes, same key");
        let different = validate(2, 2, 8, true, 8, 4, &{
            let mut d = pixels(2, 2);
            d[0] ^= 0xff;
            d
        })
        .expect("valid image");
        assert_ne!(cache_key(&image), cache_key(&different));
    }

    #[test]
    fn to_rgba_stays_within_the_declared_buffer() {
        let image = validate(4, 4, 16, true, 8, 4, &pixels(4, 4)).expect("valid image");
        let mut short = image;
        short.data.truncate(short.data.len() - 1);
        assert_eq!(to_rgba(&short), None, "a short buffer must not read past its end");
    }

    #[test]
    fn evict_keeps_the_cache_under_its_caps() {
        let dir = std::env::temp_dir().join(format!("nobody-inline-evict-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..MAX_FILES + 8 {
            std::fs::write(dir.join(format!("{i:016x}.png")), b"x").unwrap();
        }
        assert!(std::fs::read_dir(&dir).unwrap().count() > MAX_FILES);

        evict_in(&dir);
        let remaining = std::fs::read_dir(&dir).unwrap().count();
        assert!(remaining <= MAX_FILES, "count cap not enforced: {remaining}");

        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn evict_keeps_the_byte_cap() {
        let dir = std::env::temp_dir().join(format!("nobody-inline-bytes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..4 {
            std::fs::write(dir.join(format!("{i:016x}.png")), vec![0u8; 1_048_576]).unwrap();
        }
        let total: u64 = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter_map(|entry| entry.metadata().ok())
            .map(|meta| meta.len())
            .sum();
        assert!(total <= MAX_BYTES, "test setup must start under the cap");

        for i in 4..40 {
            std::fs::write(dir.join(format!("{i:016x}.png")), vec![0u8; 1_048_576]).unwrap();
        }
        evict_in(&dir);
        let after: u64 = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter_map(|entry| entry.metadata().ok())
            .map(|meta| meta.len())
            .sum();
        assert!(after <= MAX_BYTES, "byte cap not enforced: {after}");

        std::fs::remove_dir_all(dir).ok();
    }
}
