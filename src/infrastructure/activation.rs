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
        .filter(|client| {
            client["mapped"].as_bool() == Some(true)
                && client["hidden"].as_bool() == Some(false)
                && ["class", "initialClass"].into_iter().any(|field| {
                    client[field].as_str().is_some_and(|class| {
                        desktop_entry.is_some_and(|target| application_matches(class, target))
                            || application_matches(class, app)
                    })
                })
        })
        .filter_map(|client| {
            let address = client["address"].as_str()?;
            valid_address(address).then_some((
                client["focusHistoryID"].as_u64().unwrap_or(u64::MAX),
                address.to_string(),
            ))
        })
        .min_by_key(|(focus_history, _)| *focus_history)
        .map(|(_, address)| address)
}

fn application_matches(class: &str, target: &str) -> bool {
    let class = application_key(class);
    let target = application_key(target);
    target.len() >= 4
        && class.len() >= 4
        && (class == target
            || class.strip_suffix("browser") == Some(target.as_str())
            || class.ends_with(&target)
            || target.ends_with(&class))
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
}
