use crate::domain::queue::KEEP;

pub const DEFAULT_TIMEOUT_MS: i32 = 5_000;
pub const DEFAULT_MAX_VISIBLE: usize = 5;
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopRight,
    TopLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub default_timeout_ms: i32,
    pub max_visible: usize,
    pub anchor: Anchor,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_timeout_ms: DEFAULT_TIMEOUT_MS,
            max_visible: DEFAULT_MAX_VISIBLE,
            anchor: Anchor::TopRight,
        }
    }
}

pub fn parse(input: &str) -> Result<Config, String> {
    let mut config = Config::default();
    for (line_number, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) =
            line.split_once('=').ok_or_else(|| format!("linha {} sem '='", line_number + 1))?;
        let key = key.trim();
        let value = value.trim();
        match key {
            "default-timeout" => {
                let value = value.parse::<u64>().map_err(|_| {
                    format!("default-timeout inválido na linha {}", line_number + 1)
                })?;
                if value > 86_400_000 {
                    return Err(format!(
                        "default-timeout fora do limite na linha {}",
                        line_number + 1
                    ));
                }
                config.default_timeout_ms = value as i32;
            }
            "max-visible" => {
                let value = value
                    .parse::<usize>()
                    .map_err(|_| format!("max-visible inválido na linha {}", line_number + 1))?;
                if !(1..=KEEP).contains(&value) {
                    return Err(format!("max-visible fora do limite na linha {}", line_number + 1));
                }
                config.max_visible = value;
            }
            "anchor" => {
                config.anchor = match value {
                    "top-right" => Anchor::TopRight,
                    "top-left" => Anchor::TopLeft,
                    _ => return Err(format!("anchor inválido na linha {}", line_number + 1)),
                };
            }
            _ => return Err(format!("chave desconhecida na linha {}", line_number + 1)),
        }
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_stable() {
        assert_eq!(
            Config::default(),
            Config { default_timeout_ms: 5_000, max_visible: 5, anchor: Anchor::TopRight }
        );
    }

    #[test]
    fn parses_spaces_comments_duplicates_and_limits() {
        let config = parse(
            "# config\n default-timeout = 1000\nmax-visible=12\nanchor=top-left\nmax-visible = 3\n",
        )
        .unwrap();
        assert_eq!(config.default_timeout_ms, 1_000);
        assert_eq!(config.max_visible, 3);
        assert_eq!(config.anchor, Anchor::TopLeft);
    }

    #[test]
    fn rejects_invalid_values_atomically() {
        for input in [
            "default-timeout=-1",
            "default-timeout=86400001",
            "default-timeout=999999999999999999999",
            "max-visible=0",
            "max-visible=13",
            "max-visible=",
            "anchor=bottom-right",
            "unknown=value",
            "default-timeout=1000 # inline",
            "default-timeout",
        ] {
            assert!(parse(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn empty_file_keeps_defaults() {
        assert_eq!(parse("\n# only comments\n").unwrap(), Config::default());
    }
}
