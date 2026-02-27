/// Persistence layer — saves/loads `RecorderConfig` as JSON under
/// `$XDG_CONFIG_HOME/wf-recorder-ui/config.json`
/// (falls back to `~/.config/wf-recorder-ui/config.json`).
use std::fs;
use std::path::PathBuf;

use crate::config::RecorderConfig;

const APP_NAME: &str = "wf-recorder-ui";
const CONFIG_FILE: &str = "config.json";

/// Returns the path to the config file.
pub fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config")
        });

    base.join(APP_NAME).join(CONFIG_FILE)
}

/// Serialize `config` to JSON and write it to disk atomically.
/// Writes to a `.tmp` file first, then renames into place so a crash
/// mid-write never leaves a corrupt or empty config file.
pub fn save_config(config: &RecorderConfig) {
    let path = config_path();

    let parent = match path.parent() {
        Some(p) => p.to_path_buf(),
        None => {
            eprintln!("[wf-recorder-ui] Config path has no parent directory");
            return;
        }
    };

    if let Err(e) = fs::create_dir_all(&parent) {
        eprintln!("[wf-recorder-ui] Could not create config dir {}: {}", parent.display(), e);
        return;
    }

    let json = match serde_json::to_string_pretty(config) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[wf-recorder-ui] Could not serialize config: {}", e);
            return;
        }
    };

    // Write to a sibling .tmp file first, then atomically rename into place.
    // Guarantees config.json is never partially written or empty.
    let tmp_path = path.with_extension("json.tmp");

    if let Err(e) = fs::write(&tmp_path, &json) {
        eprintln!("[wf-recorder-ui] Could not write temp config {}: {}", tmp_path.display(), e);
        return;
    }

    if let Err(e) = fs::rename(&tmp_path, &path) {
        eprintln!("[wf-recorder-ui] Could not rename temp config to {}: {}", path.display(), e);
        let _ = fs::remove_file(&tmp_path);
    }
}

/// Load config from disk. Returns `RecorderConfig::default()` if the file
/// does not exist or cannot be parsed, so the app always has a valid config.
pub fn load_config() -> RecorderConfig {
    let path = config_path();

    if !path.exists() {
        return RecorderConfig::default();
    }

    match fs::read_to_string(&path) {
        Ok(json) => match serde_json::from_str::<RecorderConfig>(&json) {
            Ok(cfg) => {
                eprintln!("[wf-recorder-ui] Loaded config from {}", path.display());
                cfg
            }
            Err(e) => {
                eprintln!(
                    "[wf-recorder-ui] Config at {} is invalid ({}), using defaults.",
                    path.display(),
                    e
                );
                RecorderConfig::default()
            }
        },
        Err(e) => {
            eprintln!("[wf-recorder-ui] Could not read config file {}: {}", path.display(), e);
            RecorderConfig::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AudioMode, CaptureMode};
    use std::env;
    use tempfile::tempdir;

    #[test]
    fn round_trip() {
        let dir = tempdir().unwrap();
        // SAFETY: test-only; we accept the risk of concurrent env mutation in tests.
        unsafe { env::set_var("XDG_CONFIG_HOME", dir.path()) };

        let mut cfg = RecorderConfig::default();
        cfg.codec = "libx265".to_string();
        cfg.capture_mode = CaptureMode::Area;
        cfg.audio_mode = AudioMode::Both;
        cfg.framerate = "60".to_string();

        save_config(&cfg);

        let loaded = load_config();
        assert_eq!(loaded.codec, "libx265");
        assert_eq!(loaded.capture_mode, CaptureMode::Area);
        assert_eq!(loaded.audio_mode, AudioMode::Both);
        assert_eq!(loaded.framerate, "60");
    }

    #[test]
    fn missing_file_returns_default() {
        let dir = tempdir().unwrap();
        // SAFETY: test-only; we accept the risk of concurrent env mutation in tests.
        unsafe { env::set_var("XDG_CONFIG_HOME", dir.path()) };
        let cfg = load_config();
        assert_eq!(cfg.codec, RecorderConfig::default().codec);
    }
}
