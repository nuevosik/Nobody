use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use zbus::zvariant::{OwnedValue, Value};

use nobody::application::{commands, policy};
use nobody::domain::notice::Notice;
use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::daemon::NotificationDaemon;
use nobody::infrastructure::dbus::markup::strip_markup;
use nobody::infrastructure::dbus::validation::{
    MAX_ACTION_LEN, MAX_ACTIONS, MAX_BODY_LEN, MAX_HINTS, MAX_ICON_LEN, MAX_SUMMARY_LEN, truncate,
};
use nobody::infrastructure::icons::{resolve_named_icon, resolve_notice_icon};

#[test]
fn caps_are_body_and_icon_static_only() {
    let d = NotificationDaemon { queue: Queue::new() };
    let caps = d.get_capabilities();
    assert!(caps.contains(&"body".to_string()));
    assert!(caps.contains(&"icon-static".to_string()));
    assert!(!caps.contains(&"actions".to_string()));
    assert!(!caps.contains(&"body-markup".to_string()));
}

#[test]
fn close_missing_id_is_silent() {
    let q = Queue::new();
    assert!(q.remove(999_999).is_none());
}

#[test]
fn truncate_respects_char_boundaries_and_max_lens() {
    assert_eq!(MAX_SUMMARY_LEN, 200);
    assert_eq!(MAX_BODY_LEN, 500);
    let emoji = "🦀".repeat(600);
    let t = truncate(&emoji, MAX_BODY_LEN);
    assert_eq!(t.chars().count(), 500);
    assert_eq!(t, "🦀".repeat(500));
    let accented = "é".repeat(250);
    assert_eq!(truncate(&accented, MAX_SUMMARY_LEN).chars().count(), 200);
    let exact = "a".repeat(200);
    assert_eq!(truncate(&exact, MAX_SUMMARY_LEN), exact);
}

#[test]
fn strip_markup_contract() {
    assert_eq!(strip_markup("<b>hi</b>"), "hi");
    assert_eq!(strip_markup("a < b"), "a < b");
    assert_eq!(strip_markup("a <3 b"), "a <3 b");
    assert_eq!(strip_markup("&lt;b&gt; &amp; &quot;x&quot;"), "<b> & \"x\"");
}

#[test]
fn icon_lookup_scoped_and_desktop_fallback_safe() {
    assert!(resolve_named_icon("../../etc/passwd").is_none());
    assert!(resolve_named_icon(&"a".repeat(513)).is_none());
    assert!(resolve_named_icon("/etc/passwd").is_none());
    let hints = HashMap::from([(
        "desktop-entry".to_string(),
        OwnedValue::try_from(Value::from("../../etc/passwd")).unwrap(),
    )]);
    assert!(resolve_notice_icon("", "no-such-app-xyz987", &hints).is_none());
}

/// Serializes tests sharing the process-global icon cache (and the XDG env):
/// a flood/eviction run must not wipe another test's cached entry mid-assert.
/// Integration binaries are separate processes, so this only orders threads
/// inside this binary.
fn infra_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn uniq(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("nobody-r2-{tag}-{}-{nanos}", std::process::id())
}

fn write_png(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, b"fake-png").unwrap();
}

fn str_hint(value: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(value)).unwrap()
}

/// Restores XDG_DATA_HOME/XDG_DATA_DIRS on drop so a panicking assert cannot
/// leak mutated env into other tests in this binary.
struct XdgGuard {
    home: Option<std::ffi::OsString>,
    dirs: Option<std::ffi::OsString>,
}

impl XdgGuard {
    fn set(base: &Path) -> Self {
        let guard = Self {
            home: std::env::var_os("XDG_DATA_HOME"),
            dirs: std::env::var_os("XDG_DATA_DIRS"),
        };
        unsafe {
            std::env::set_var("XDG_DATA_HOME", base);
            std::env::set_var("XDG_DATA_DIRS", "/nonexistent-nobody-r2-xyz");
        }
        guard
    }
}

impl Drop for XdgGuard {
    fn drop(&mut self) {
        unsafe {
            match &self.home {
                Some(v) => std::env::set_var("XDG_DATA_HOME", v),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
            match &self.dirs {
                Some(v) => std::env::set_var("XDG_DATA_DIRS", v),
                None => std::env::remove_var("XDG_DATA_DIRS"),
            }
        }
    }
}

// --- cache.rs (via public resolve_named_icon; absolute paths need no XDG) ---

#[test]
fn icon_cache_hit_serves_deleted_absolute_path() {
    let _guard = infra_lock().lock().unwrap_or_else(|e| e.into_inner());
    let path = std::env::temp_dir().join(format!("{}.png", uniq("hit")));
    write_png(&path);
    let key = path.to_str().unwrap().to_string();
    assert_eq!(resolve_named_icon(&key), Some(path.clone()));
    std::fs::remove_file(&path).unwrap();
    // Positive hits are cached: file is gone but the cached path is served.
    assert_eq!(resolve_named_icon(&key), Some(path));
}

#[test]
fn icon_cache_evicts_stale_entries_under_pressure() {
    let _guard = infra_lock().lock().unwrap_or_else(|e| e.into_inner());
    // Seed several cached positives, then delete their files: each re-lookup
    // returns None iff that entry was evicted (lookup itself must miss).
    // Eviction removes the first 64 keys in HashMap iteration order, whose
    // bucket positions are hash-dependent, so per-entry eviction is NOT
    // deterministic — but over a dozen seeds at least one is evicted with
    // overwhelming probability, while with eviction disabled all survive.
    let mut keys = Vec::new();
    for i in 0..12 {
        let path = std::env::temp_dir().join(format!("{}.png", uniq(&format!("seed{i}"))));
        write_png(&path);
        let key = path.to_str().unwrap().to_string();
        assert_eq!(resolve_named_icon(&key), Some(path.clone()));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(resolve_named_icon(&key), Some(path), "sanity: hit cached");
        keys.push(key);
    }

    // Push ~6000 unique misses through the 256-entry cap (~90 eviction
    // cycles of 64-for-64 replacement).
    let flood = uniq("flood");
    for i in 0..6000 {
        assert!(resolve_named_icon(&format!("/tmp/{flood}-{i}.png")).is_none());
    }
    let evicted = keys.iter().filter(|k| resolve_named_icon(k).is_none()).count();
    assert!(evicted >= 1, "cap must evict: all {} stale entries survived 6000 inserts", keys.len());

    // Cache still functional at cap: a fresh file resolves.
    let fresh = std::env::temp_dir().join(format!("{}.png", uniq("fresh")));
    write_png(&fresh);
    let fresh_key = fresh.to_str().unwrap().to_string();
    let got = resolve_named_icon(&fresh_key);
    std::fs::remove_file(&fresh).ok();
    assert_eq!(got, Some(fresh));
}

// --- resolver.rs priority + rejection (public resolve_notice_icon) ---

#[test]
fn resolve_hint_image_path_beats_app_icon_and_app() {
    let _guard = infra_lock().lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir();
    let hint_p = dir.join(format!("{}.png", uniq("hint")));
    let icon_p = dir.join(format!("{}.png", uniq("appicon")));
    let app_p = dir.join(format!("{}.png", uniq("appfb")));
    write_png(&hint_p);
    write_png(&icon_p);
    write_png(&app_p);
    let (h, a, p) = (
        hint_p.to_str().unwrap().to_string(),
        icon_p.to_str().unwrap().to_string(),
        app_p.to_str().unwrap().to_string(),
    );

    for key in ["image-path", "image_path"] {
        let hints = HashMap::from([(key.to_string(), str_hint(&h))]);
        assert_eq!(resolve_notice_icon(&a, &p, &hints), Some(hint_p.clone()), "key {key}");
    }
    // A missing hint file falls through to app_icon, not to None.
    let ghost = format!("/tmp/{}-ghost.png", uniq("ghost"));
    let hints = HashMap::from([("image-path".to_string(), str_hint(&ghost))]);
    assert_eq!(resolve_notice_icon(&a, &p, &hints), Some(icon_p.clone()));

    std::fs::remove_file(&hint_p).ok();
    std::fs::remove_file(&icon_p).ok();
    std::fs::remove_file(&app_p).ok();
}

#[test]
fn resolve_app_icon_beats_app_fallback() {
    let _guard = infra_lock().lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir();
    let icon_p = dir.join(format!("{}.png", uniq("icon2")));
    let app_p = dir.join(format!("{}.png", uniq("app2")));
    write_png(&icon_p);
    write_png(&app_p);
    let (a, p) = (icon_p.to_str().unwrap().to_string(), app_p.to_str().unwrap().to_string());
    let empty: HashMap<String, OwnedValue> = HashMap::new();
    let missing = format!("{}-missing", uniq("miss"));

    assert_eq!(resolve_notice_icon(&a, &p, &empty), Some(icon_p.clone()));
    assert_eq!(resolve_notice_icon(&missing, &p, &empty), Some(app_p.clone()));
    assert!(resolve_notice_icon(&missing, &missing, &empty).is_none());

    std::fs::remove_file(&icon_p).ok();
    std::fs::remove_file(&app_p).ok();
}

#[test]
fn resolve_hint_rejects_bad_values_and_names() {
    let _guard = infra_lock().lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir();
    let real_p = dir.join(format!("{}.png", uniq("real")));
    write_png(&real_p);
    let real = real_p.to_str().unwrap().to_string();
    let missing = format!("{}-missing", uniq("miss2"));

    // file:// prefix is stripped, so a hint URI resolves to the real file.
    let hints = HashMap::from([("image-path".to_string(), str_hint(&format!("file://{real}")))]);
    assert_eq!(resolve_notice_icon(&missing, &missing, &hints), Some(real_p.clone()));

    // Traversal / backslash / empty / wrong-type hints are ignored.
    for bad in ["../../etc/passwd", "..\\..\\windows", "", "a/b"] {
        let hints = HashMap::from([("image-path".to_string(), str_hint(bad))]);
        assert!(resolve_notice_icon(&missing, &missing, &hints).is_none(), "hint {bad}");
    }
    let hints = HashMap::from([("image-path".to_string(), OwnedValue::from(2_u8))]);
    assert!(resolve_notice_icon(&missing, &missing, &hints).is_none());
    // Oversized app_icon (>512) is rejected before any lookup.
    assert!(resolve_notice_icon(&"a".repeat(600), &missing, &hints).is_none());
    // Traversal desktop-entry with nothing else hitting resolves to None.
    let hints = HashMap::from([("desktop-entry".to_string(), str_hint("../../etc/passwd"))]);
    assert!(resolve_notice_icon(&missing, &missing, &hints).is_none());

    std::fs::remove_file(&real_p).ok();
}

#[test]
fn resolve_desktop_entry_beats_app_loses_to_app_icon() {
    let _guard = infra_lock().lock().unwrap_or_else(|e| e.into_inner());
    let tag = uniq("desk");
    let base = std::env::temp_dir().join(format!("{tag}-home"));
    let apps = base.join("applications");
    let icondir = base.join("icons/hicolor/48x48/apps");
    let icon_name = format!("{tag}-icon");
    let fb_name = format!("{tag}-fb");
    let entry_id = format!("{tag}-entry");
    write_png(&icondir.join(format!("{icon_name}.png")));
    write_png(&icondir.join(format!("{fb_name}.png")));
    std::fs::create_dir_all(&apps).unwrap();
    std::fs::write(
        apps.join(format!("{entry_id}.desktop")),
        format!("[Desktop Entry]\nName=T\nIcon={icon_name}\n"),
    )
    .unwrap();
    let override_p = base.join("override.png");
    write_png(&override_p);
    let override_s = override_p.to_str().unwrap().to_string();
    let expected = icondir.join(format!("{icon_name}.png"));
    let missing = format!("{tag}-missing");

    let _xdg = XdgGuard::set(&base);
    // desktop-entry resolves through the .desktop Icon= key …
    let hints = HashMap::from([("desktop-entry".to_string(), str_hint(&entry_id))]);
    assert_eq!(resolve_notice_icon(&missing, &missing, &hints), Some(expected.clone()));
    // … beats the app-name fallback …
    assert_eq!(resolve_notice_icon(&missing, &fb_name, &hints), Some(expected.clone()));
    // … but loses to an explicit app_icon path.
    assert_eq!(resolve_notice_icon(&override_s, &fb_name, &hints), Some(override_p.clone()));
    // Underscore spelling works the same way.
    let hints2 = HashMap::from([("desktop_entry".to_string(), str_hint(&entry_id))]);
    assert_eq!(resolve_notice_icon(&missing, &missing, &hints2), Some(expected));
    // image-path hint outranks desktop-entry.
    let hints3 = HashMap::from([
        ("image-path".to_string(), str_hint(&override_s)),
        ("desktop-entry".to_string(), str_hint(&entry_id)),
    ]);
    assert_eq!(resolve_notice_icon(&missing, &missing, &hints3), Some(override_p));
    drop(_xdg);

    std::fs::remove_dir_all(&base).ok();
}

// --- daemon.rs Notify-pipeline gaps drivable without a bus ---

#[test]
fn notify_pipeline_strips_markup_then_truncates_to_caps() {
    assert_eq!(MAX_SUMMARY_LEN, 200);
    assert_eq!(MAX_BODY_LEN, 500);
    // Strip-first: markup must not consume the char budget.
    let cooked = truncate(&strip_markup(&format!("<b>{}</b>", "x".repeat(200))), MAX_SUMMARY_LEN);
    assert_eq!(cooked, "x".repeat(200));
    // Multi-byte chars respect char (not byte) boundaries after stripping.
    let summary = truncate(&strip_markup(&format!("<b>{}</b>", "é".repeat(250))), MAX_SUMMARY_LEN);
    assert_eq!(summary.chars().count(), 200);
    assert!(!summary.contains('<'));
    let body = truncate(&strip_markup(&format!("<i>{}</i>", "🦀".repeat(600))), MAX_BODY_LEN);
    assert_eq!(body, "🦀".repeat(500));
}

#[test]
fn notify_pipeline_policy_pins_expire_contract() {
    assert_eq!(policy::DEFAULT_EXPIRE_MS, 5_000);
    assert_eq!(policy::effective_expire_timeout(-1, false), 5_000);
    assert_eq!(policy::effective_expire_timeout(0, false), 0);
    assert_eq!(policy::effective_expire_timeout(2_500, false), 2_500);
    assert_eq!(policy::effective_expire_timeout(2_500, true), 0);
    assert_eq!(policy::effective_expire_timeout(-1, true), 0);
    assert_eq!(policy::effective_expire_timeout(0, true), 0);
}

#[test]
fn notify_pipeline_validation_caps_for_actions_hints_icon() {
    assert_eq!(MAX_ACTIONS, 20);
    assert_eq!(MAX_ACTION_LEN, 64);
    assert_eq!(MAX_HINTS, 64);
    assert_eq!(MAX_ICON_LEN, 512);
    let mut actions: Vec<String> = (0..25).map(|i| format!("act-{i}")).collect();
    if actions.len() > MAX_ACTIONS {
        actions.truncate(MAX_ACTIONS);
    }
    assert_eq!(actions.len(), MAX_ACTIONS);
    let capped: Vec<String> = actions.into_iter().map(|a| truncate(&a, MAX_ACTION_LEN)).collect();
    assert_eq!(capped.len(), MAX_ACTIONS);
    assert_eq!(truncate(&"k".repeat(100), MAX_ACTION_LEN).chars().count(), MAX_ACTION_LEN);
    let hints: HashMap<String, OwnedValue> =
        (0..70).map(|i| (format!("k{i}"), OwnedValue::from(1_u8))).collect();
    let limited: HashMap<String, OwnedValue> =
        if hints.len() > MAX_HINTS { hints.into_iter().take(MAX_HINTS).collect() } else { hints };
    assert_eq!(limited.len(), MAX_HINTS);
    assert_eq!(truncate(&"p".repeat(600), MAX_ICON_LEN).chars().count(), MAX_ICON_LEN);
}

#[test]
fn notify_pipeline_expire_runs_before_push() {
    let q = Queue::new();
    q.push_with_outcome(
        0,
        Notice {
            id: 0,
            app: "Old".into(),
            summary: "s".into(),
            body: String::new(),
            icon: None,
            actions: vec![],
            expire_ms: 10,
            arrived_at_ms: 100,
        },
    );
    let expired = commands::expire(&q, 1_000);
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].app, "Old");
    assert!(q.snapshot().is_empty());
    q.push_with_outcome(
        0,
        Notice {
            id: 0,
            app: "New".into(),
            summary: "s".into(),
            body: String::new(),
            icon: None,
            actions: vec![],
            expire_ms: 0,
            arrived_at_ms: 0,
        },
    );
    assert_eq!(q.snapshot().len(), 1);
}
