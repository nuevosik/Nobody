use std::collections::HashMap;
use std::path::PathBuf;

use zbus::zvariant::OwnedValue;

use super::desktop::icon_from_desktop;
use super::lookup::resolve_named_icon;

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
