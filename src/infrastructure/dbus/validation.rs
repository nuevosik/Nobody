use std::collections::HashMap;
use std::collections::HashSet;
use zbus::zvariant::OwnedValue;

use crate::domain::notice::is_valid_stack_tag;

pub const MAX_SUMMARY_LEN: usize = 200;
pub const MAX_BODY_LEN: usize = 500;
pub const MAX_ACTIONS: usize = 20;
pub const MAX_ACTION_LEN: usize = 64;
pub const MAX_HINTS: usize = 64;
pub const MAX_ICON_LEN: usize = 512;

pub(crate) fn stack_tag(hints: &HashMap<String, OwnedValue>) -> Option<String> {
    ["x-dunst-stack-tag", "x-canonical-private-synchronous"]
        .into_iter()
        .filter_map(|key| hints.get(key))
        .filter_map(|value| String::try_from(value.try_clone().ok()?).ok())
        .find(|tag| is_valid_stack_tag(tag))
}

pub(crate) fn progress(hints: &HashMap<String, OwnedValue>) -> Option<u8> {
    let value = hints.get("value")?.try_clone().ok()?;
    let value = i32::try_from(value).ok()?;
    (0..=100).contains(&value).then_some(value as u8)
}

pub(crate) fn actions(input: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for pair in input.as_chunks::<2>().0 {
        if output.len() + 2 > MAX_ACTIONS {
            break;
        }
        let key = &pair[0];
        if key.is_empty() || key.chars().count() > MAX_ACTION_LEN || !seen.insert(key.clone()) {
            continue;
        }
        output.push(key.clone());
        output.push(truncate(&pair[1], MAX_ACTION_LEN));
    }
    output
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

pub(crate) fn is_critical(hints: &HashMap<String, OwnedValue>) -> bool {
    if let Some(v) = hints.get("urgency") {
        if let Ok(cloned) = v.try_clone()
            && let Ok(b) = u8::try_from(cloned)
        {
            return b >= 2;
        }
        if let Ok(cloned) = v.try_clone()
            && let Ok(n) = i32::try_from(cloned)
        {
            return n >= 2;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_critical_urgency() {
        let hints = HashMap::from([("urgency".into(), OwnedValue::from(2_u8))]);

        assert!(is_critical(&hints));
    }

    #[test]
    fn critical_threshold_covers_u8_i32_and_missing() {
        for (v, expected) in [(0_u8, false), (1_u8, false), (2_u8, true), (3_u8, true)] {
            let hints = HashMap::from([("urgency".into(), OwnedValue::from(v))]);
            assert_eq!(is_critical(&hints), expected, "u8 {v}");
        }
        for (v, expected) in [(0_i32, false), (1_i32, false), (2_i32, true), (5_i32, true)] {
            let hints = HashMap::from([("urgency".into(), OwnedValue::from(v))]);
            assert_eq!(is_critical(&hints), expected, "i32 {v}");
        }
        assert!(!is_critical(&HashMap::new()));
    }

    #[test]
    fn truncate_limits() {
        assert_eq!(truncate("abcdef", 3), "abc");
    }

    #[test]
    fn stack_tag_accepts_valid_values_with_precedence_and_ignores_invalid_hints() {
        let canonical = OwnedValue::try_from(zbus::zvariant::Value::from("canonical")).unwrap();
        let dunst = OwnedValue::try_from(zbus::zvariant::Value::from("dunst")).unwrap();
        let hints = HashMap::from([
            ("x-canonical-private-synchronous".into(), canonical.clone()),
            ("x-dunst-stack-tag".into(), dunst),
        ]);
        assert_eq!(stack_tag(&hints).as_deref(), Some("dunst"));

        let invalid_dunst = OwnedValue::from(7_u8);
        let hints = HashMap::from([
            ("x-dunst-stack-tag".into(), invalid_dunst),
            ("x-canonical-private-synchronous".into(), canonical),
        ]);
        assert_eq!(stack_tag(&hints).as_deref(), Some("canonical"));

        for tag in ["", &"🦀".repeat(crate::domain::notice::MAX_STACK_TAG_LEN + 1)] {
            let hints = HashMap::from([(
                "x-dunst-stack-tag".into(),
                OwnedValue::try_from(zbus::zvariant::Value::from(tag)).unwrap(),
            )]);
            assert!(stack_tag(&hints).is_none(), "tag should be ignored: {tag:?}");
        }

        let wrong_type = HashMap::from([("x-dunst-stack-tag".into(), OwnedValue::from(7_u8))]);
        assert!(stack_tag(&wrong_type).is_none());

        let whitespace = HashMap::from([(
            "x-dunst-stack-tag".into(),
            OwnedValue::try_from(zbus::zvariant::Value::from(" ")).unwrap(),
        )]);
        assert_eq!(stack_tag(&whitespace).as_deref(), Some(" "));
    }

    #[test]
    fn progress_accepts_only_int32_values_from_zero_to_one_hundred() {
        for (value, expected) in
            [(0_i32, Some(0)), (50, Some(50)), (100, Some(100)), (-1, None), (101, None)]
        {
            let hints = HashMap::from([("value".into(), OwnedValue::from(value))]);
            assert_eq!(progress(&hints), expected);
        }
        let string = HashMap::from([(
            "value".into(),
            OwnedValue::try_from(zbus::zvariant::Value::from("50")).unwrap(),
        )]);
        assert_eq!(progress(&string), None);
        assert_eq!(progress(&HashMap::new()), None);
        let unsigned = HashMap::from([("value".into(), OwnedValue::from(50_u8))]);
        assert_eq!(progress(&unsigned), None);
    }

    #[test]
    fn actions_keep_complete_valid_unique_pairs_and_cap_pairs() {
        let mut input = vec![
            "default".into(),
            "Open".into(),
            "default".into(),
            "Duplicate".into(),
            "".into(),
            "Empty".into(),
            "too-long-key".repeat(MAX_ACTION_LEN),
            "Discard".into(),
            "other".into(),
            "x".repeat(MAX_ACTION_LEN + 1),
        ];
        input.push("orphan".into());
        let parsed = actions(&input);
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0], "default");
        assert_eq!(parsed[1], "Open");
        assert_eq!(parsed[2], "other");
        assert_eq!(parsed[3].chars().count(), MAX_ACTION_LEN);

        let many: Vec<String> = (0..(MAX_ACTIONS + 2))
            .flat_map(|i| [format!("key-{i}"), format!("label-{i}")])
            .collect();
        assert_eq!(actions(&many).len(), MAX_ACTIONS);
    }
}
