use std::collections::HashMap;

use zbus::zvariant::{OwnedValue, Value};

use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::daemon::NotificationDaemon;
use nobody::infrastructure::dbus::markup::strip_markup;
use nobody::infrastructure::dbus::validation::{MAX_BODY_LEN, MAX_SUMMARY_LEN, truncate};
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
