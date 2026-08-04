//! Persistent configuration (`config.json`), PID file, and the global-mute hotkey spec.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// 0.0–1.0 master volume.
    pub volume: f32,
    pub muted: bool,
    /// e.g. "Ctrl+Shift+F12". Empty string disables the hotkey.
    pub hotkey: String,
    /// Override the output buffer request (frames). None = negotiate 128→256→default.
    pub buffer_frames: Option<u32>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            volume: 0.8,
            muted: false,
            hotkey: "Ctrl+Shift+F12".to_string(),
            buffer_frames: None,
        }
    }
}

pub fn dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("aural")
}

pub fn path() -> PathBuf {
    dir().join("config.json")
}

pub fn pid_path() -> PathBuf {
    dir().join("aural.pid")
}

pub fn load() -> Config {
    let Ok(text) = std::fs::read_to_string(path()) else {
        return Config::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save(config: &Config) -> Result<()> {
    let dir = dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let text = serde_json::to_string_pretty(config)?;
    std::fs::write(path(), text).context("writing config")?;
    Ok(())
}

/// Update one field on disk and return the new config (used by mute/volume commands).
pub fn update(f: impl FnOnce(&mut Config)) -> Result<Config> {
    let mut config = load();
    f(&mut config);
    save(&config)?;
    Ok(config)
}

/// The daemon polls this to pick up CLI changes (mute/volume) within a second.
pub fn mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

/// Parse "Ctrl+Shift+F12" into (modifiers, vk) as used by RegisterHotKey.
/// Returns None for an empty/unparseable spec.
pub fn parse_hotkey(spec: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut key: Option<u32> = None;
    for part in spec.split('+').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= 0x0002, // MOD_CONTROL
            "shift" => mods |= 0x0004,            // MOD_SHIFT
            "alt" => mods |= 0x0001,              // MOD_ALT
            "win" => mods |= 0x0008,              // MOD_WIN
            k if k.len() == 1 => {
                let c = k.chars().next().unwrap();
                if c.is_ascii_alphanumeric() {
                    key = Some(c.to_ascii_uppercase() as u32);
                } else {
                    return None;
                }
            }
            k if k.starts_with('f') => {
                let n: u32 = k[1..].parse().ok()?;
                if !(1..=24).contains(&n) {
                    return None;
                }
                key = Some(0x70 + n - 1); // VK_F1 = 0x70
            }
            _ => return None,
        }
    }
    key.map(|k| (mods, k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_hotkey() {
        assert_eq!(
            parse_hotkey("Ctrl+Shift+F12"),
            Some((0x0002 | 0x0004, 0x7B))
        );
    }

    #[test]
    fn parses_letters_and_rejects_junk() {
        assert_eq!(
            parse_hotkey("ctrl+alt+k"),
            Some((0x0001 | 0x0002, b'K' as u32))
        );
        assert_eq!(parse_hotkey(""), None);
        assert_eq!(parse_hotkey("ctrl+"), None);
        assert_eq!(parse_hotkey("F0"), None);
        assert_eq!(parse_hotkey("F25"), None);
    }
}
