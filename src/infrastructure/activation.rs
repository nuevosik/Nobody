use std::process::Command;
use std::time::Duration;

use super::fullscreen::command_output;

const TIMEOUT: Duration = Duration::from_millis(500);

pub fn focus_application(desktop_entry: Option<String>, app: String) {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return;
    }
    let Some(clients) = command_output(Command::new("hyprctl").args(["-j", "clients"]), TIMEOUT)
    else {
        return;
    };
    let Some(address) = window_address(&clients, desktop_entry.as_deref(), &app) else {
        return;
    };
    let expression = format!(r#"hl.dsp.focus({{ window = "address:{address}" }})"#);
    let _ = command_output(Command::new("hyprctl").args(["dispatch", &expression]), TIMEOUT);
}

fn window_address(clients: &[u8], desktop_entry: Option<&str>, app: &str) -> Option<String> {
    let clients = serde_json::from_slice::<serde_json::Value>(clients).ok()?;
    clients
        .as_array()?
        .iter()
        .filter_map(|client| {
            if client["mapped"].as_bool() != Some(true) || client["hidden"].as_bool() != Some(false)
            {
                return None;
            }
            let address = client["address"].as_str()?;
            if !valid_address(address) {
                return None;
            }
            let rank = ["class", "initialClass"]
                .into_iter()
                .filter_map(|field| client[field].as_str())
                .filter_map(|class| best_match_rank(class, desktop_entry, app))
                .min()?;
            let focus = client["focusHistoryID"].as_u64().unwrap_or(u64::MAX);
            Some((rank, focus, address))
        })
        .min_by_key(|(rank, focus, _)| (*rank, *focus))
        .map(|(_, _, address)| address.to_string())
}

fn best_match_rank(class: &str, desktop_entry: Option<&str>, app: &str) -> Option<u8> {
    desktop_entry
        .into_iter()
        .chain(std::iter::once(app))
        .filter_map(|target| match_rank(class, target))
        .min()
}

fn match_rank(class: &str, target: &str) -> Option<u8> {
    let class_key = application_key(class);
    let target_key = application_key(target);
    if class_key.len() < 4 || target_key.len() < 4 {
        return None;
    }
    if class_key == target_key {
        return Some(0);
    }
    if class_key.strip_suffix("browser") == Some(target_key.as_str()) {
        return Some(1);
    }
    let suffix_match = token_suffixes(class).iter().any(|key| key == &target_key)
        || token_suffixes(target).iter().any(|key| key == &class_key);
    suffix_match.then_some(2)
}

fn token_suffixes(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut suffix = String::new();
    for token in value.split(|character: char| !character.is_alphanumeric()).rev() {
        if token.is_empty() {
            continue;
        }
        suffix.insert_str(0, &token.to_lowercase());
        out.push(suffix.clone());
    }
    out
}

fn application_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn valid_address(address: &str) -> bool {
    address.strip_prefix("0x").is_some_and(|hex| {
        !hex.is_empty() && hex.chars().all(|character| character.is_ascii_hexdigit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_most_recent_visible_window_for_the_application() {
        let clients = br#"[
            {"address":"0x111","mapped":true,"hidden":false,"class":"brave-browser","initialClass":"brave-browser","focusHistoryID":4},
            {"address":"0x222","mapped":true,"hidden":false,"class":"brave-browser","initialClass":"brave-browser","focusHistoryID":1},
            {"address":"0x333","mapped":true,"hidden":true,"class":"brave-browser","initialClass":"brave-browser","focusHistoryID":0},
            {"address":"invalid","mapped":true,"hidden":false,"class":"brave-browser","initialClass":"brave-browser","focusHistoryID":0}
        ]"#;

        assert_eq!(
            window_address(clients, Some("brave-browser"), "Brave").as_deref(),
            Some("0x222")
        );
        assert_eq!(
            window_address(clients, Some("com.brave.Browser"), "Unknown").as_deref(),
            Some("0x222")
        );
        assert_eq!(
            window_address(clients, Some("invalid.desktop"), "Brave").as_deref(),
            Some("0x222")
        );
        assert_eq!(window_address(clients, None, "unknown"), None);
    }

    #[test]
    fn ranks_exact_over_suffix_and_rejects_glued_prefixes() {
        let clients = br#"[
            {"address":"0x111","mapped":true,"hidden":false,"class":"xterm","initialClass":"xterm","focusHistoryID":0},
            {"address":"0x222","mapped":true,"hidden":false,"class":"org.gnome.Terminal","initialClass":"org.gnome.Terminal","focusHistoryID":5},
            {"address":"0x333","mapped":true,"hidden":false,"class":"konsole","initialClass":"konsole","focusHistoryID":9}
        ]"#;

        assert_eq!(window_address(clients, None, "term"), None);
        assert_eq!(window_address(clients, None, "terminal").as_deref(), Some("0x222"));
        assert_eq!(window_address(clients, None, "konsole").as_deref(), Some("0x333"));
    }

    #[test]
    fn browser_suffix_and_token_boundaries_match_expected_windows() {
        let clients = br#"[
            {"address":"0x111","mapped":true,"hidden":false,"class":"google-chrome","initialClass":"google-chrome","focusHistoryID":3},
            {"address":"0x222","mapped":true,"hidden":false,"class":"brave-browser","initialClass":"brave-browser","focusHistoryID":2},
            {"address":"0x333","mapped":true,"hidden":false,"class":"vscode","initialClass":"vscode","focusHistoryID":0}
        ]"#;

        assert_eq!(window_address(clients, None, "chrome").as_deref(), Some("0x111"));
        assert_eq!(window_address(clients, None, "brave").as_deref(), Some("0x222"));
        assert_eq!(window_address(clients, None, "code"), None);
    }

    #[test]
    fn exact_match_beats_loose_match_with_better_focus_history() {
        let clients = br#"[
            {"address":"0x111","mapped":true,"hidden":false,"class":"org.gnome.Terminal","initialClass":"org.gnome.Terminal","focusHistoryID":0},
            {"address":"0x222","mapped":true,"hidden":false,"class":"terminal","initialClass":"terminal","focusHistoryID":7}
        ]"#;

        assert_eq!(window_address(clients, None, "terminal").as_deref(), Some("0x222"));
    }

    #[test]
    fn desktop_entry_and_app_name_are_both_considered() {
        let clients = br#"[
            {"address":"0x111","mapped":true,"hidden":false,"class":"brave-browser","initialClass":"brave-browser","focusHistoryID":0}
        ]"#;

        assert_eq!(
            window_address(clients, Some("com.brave.Browser"), "Unknown").as_deref(),
            Some("0x111")
        );
        assert_eq!(
            window_address(clients, Some("com.brave.Browser"), "Brave").as_deref(),
            Some("0x111")
        );
    }
}
