use nobody::domain::queue::Queue;
use nobody::infrastructure::dbus::daemon::NotificationDaemon;

#[test]
fn caps_are_body_and_icon_static_only() {
    let d = NotificationDaemon { queue: Queue::new() };
    let caps = d.get_capabilities();
    assert!(caps.contains(&"body".to_string()));
    assert!(caps.contains(&"icon-static".to_string()));
    assert!(!caps.contains(&"actions".to_string()));
    assert!(!caps.contains(&"body-markup".to_string()));
}
