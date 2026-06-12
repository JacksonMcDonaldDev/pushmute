//! Persistent configuration under `$XDG_CONFIG_HOME/pushmute/config.toml`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The PipeWire `node.name` of PushMute's own virtual source. Fixed so it is stable
/// across runs and easy to exclude when listing physical mics.
pub const PUSHMUTE_NODE_NAME: &str = "pushmute";
pub const PUSHMUTE_DESCRIPTION: &str = "PushMute";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// `node.name` of the physical capture device to route from.
    pub physical_mic: Option<String>,
    /// evdev keycodes that must *all* be held for the hotkey (a chord). A
    /// single entry is a plain single-key bind.
    #[serde(default)]
    pub hotkey_keys: Vec<u16>,
    /// Optional specific `/dev/input/eventN` to listen on. `None` = all keyboards.
    pub hotkey_device: Option<String>,
    /// Whether to set `pushmute` as the default source on startup.
    #[serde(default = "default_true")]
    pub set_default: bool,
    /// The default source recorded before PushMute changed it, for restoration.
    pub previous_default: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Config {
            physical_mic: None,
            hotkey_keys: Vec::new(),
            hotkey_device: None,
            set_default: true,
            previous_default: None,
        }
    }
}

impl Config {
    pub fn dir() -> PathBuf {
        if let Ok(x) = std::env::var("XDG_CONFIG_HOME") {
            if !x.is_empty() {
                return PathBuf::from(x).join("pushmute");
            }
        }
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".config").join("pushmute")
    }

    pub fn path() -> PathBuf {
        Self::dir().join("config.toml")
    }

    pub fn load() -> Result<Config> {
        let path = Self::path();
        match std::fs::read_to_string(&path) {
            Ok(s) => toml::from_str(&s)
                .with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating {}", dir.display()))?;
        let s = toml::to_string_pretty(self)?;
        std::fs::write(Self::path(), s)
            .with_context(|| format!("writing {}", Self::path().display()))?;
        Ok(())
    }
}
