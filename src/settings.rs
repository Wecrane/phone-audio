use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Last used device serial (USB serial or `ip:port`).
    pub serial: String,
    /// Target audio buffer in milliseconds.
    pub buffer_ms: u32,
    /// opus | aac | flac | raw
    pub codec: String,
    /// output | playback
    pub source: String,
    /// Keep the phone speaker playing while capturing (`--audio-dup`).
    pub audio_dup: bool,
    /// Audio bitrate in kbps (ignored by the `raw` codec).
    pub bitrate_kbps: u32,
    pub auto_reconnect: bool,
    /// Start streaming the saved device automatically on launch.
    pub auto_connect: bool,
    /// Closing the window hides it to the system tray instead of exiting.
    pub close_to_tray: bool,
    /// The user has answered the first-close tray prompt (or used the setting).
    pub tray_choice_made: bool,
    /// Last window size (logical points), restored on the next launch.
    pub window_width: f32,
    pub window_height: f32,
    /// Preferred computer output device (empty = system default).
    pub output_device: String,
    /// UI theme: dark | light.
    pub theme: String,
    pub pair_addr: String,
    pub pair_code: String,
    pub connect_addr: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            serial: String::new(),
            buffer_ms: 50,
            codec: "opus".to_string(),
            source: "output".to_string(),
            audio_dup: false,
            bitrate_kbps: 128,
            auto_reconnect: true,
            auto_connect: false,
            close_to_tray: true,
            tray_choice_made: false,
            window_width: 920.0,
            window_height: 800.0,
            output_device: String::new(),
            theme: "system".to_string(),
            pair_addr: String::new(),
            pair_code: String::new(),
            connect_addr: String::new(),
        }
    }
}

impl Settings {
    /// Clamp / whitelist values loaded from disk so the UI and the scrcpy
    /// arguments can never disagree.
    pub fn normalize(&mut self) {
        self.buffer_ms = self.buffer_ms.clamp(20, 200);
        self.bitrate_kbps = self.bitrate_kbps.clamp(32, 512);
        self.codec = match self.codec.as_str() {
            "aac" | "flac" | "raw" | "opus" => self.codec.clone(),
            _ => "opus".to_string(),
        };
        self.source = match self.source.as_str() {
            "playback" => "playback".to_string(),
            _ => "output".to_string(),
        };
        self.serial = self.serial.trim().to_string();
        self.output_device = self.output_device.trim().to_string();
        self.theme = match self.theme.as_str() {
            "light" => "light".to_string(),
            "dark" => "dark".to_string(),
            _ => "system".to_string(),
        };
        if self.source != "playback" {
            self.audio_dup = false;
        }
        self.window_width = self.window_width.clamp(640.0, 3840.0);
        self.window_height = self.window_height.clamp(520.0, 2160.0);
    }
}

pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("dev", "PhoneAudio", "PhoneAudio")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(text) = fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(mut settings) => {
                settings.normalize();
                settings
            }
            Err(error) => {
                // Keep the broken file for debugging instead of silently
                // discarding the user's configuration.
                let backup = path.with_extension("json.bad");
                let _ = fs::rename(&path, &backup);
                eprintln!(
                    "[phone-audio] 配置解析失败（{error}），已备份到 {}",
                    backup.display()
                );
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let Some(path) = config_path() else {
            return;
        };
        let Some(dir) = path.parent() else {
            return;
        };
        let _ = fs::create_dir_all(dir);
        let Ok(json) = serde_json::to_string_pretty(self) else {
            return;
        };

        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, &json).is_err() {
            return;
        }
        if fs::rename(&tmp, &path).is_err() {
            // Antivirus / network shares may refuse rename; fall back to a
            // direct write rather than losing the settings.
            let _ = fs::write(&path, &json);
            let _ = fs::remove_file(&tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_unknown_values() {
        let mut settings = Settings {
            codec: "weird".to_string(),
            source: "bogus".to_string(),
            buffer_ms: 5,
            bitrate_kbps: 9999,
            audio_dup: true,
            ..Default::default()
        };
        settings.normalize();
        assert_eq!(settings.codec, "opus");
        assert_eq!(settings.source, "output");
        assert_eq!(settings.buffer_ms, 20);
        assert_eq!(settings.bitrate_kbps, 512);
        assert!(!settings.audio_dup, "dup only valid with playback source");
    }

    #[test]
    fn round_trips_through_json() {
        let mut settings = Settings {
            serial: "R58M1234".to_string(),
            codec: "aac".to_string(),
            source: "playback".to_string(),
            audio_dup: true,
            output_device: "Speakers".to_string(),
            ..Default::default()
        };
        settings.normalize();
        let json = serde_json::to_string(&settings).unwrap();
        let mut back: Settings = serde_json::from_str(&json).unwrap();
        back.normalize();
        assert_eq!(back.serial, "R58M1234");
        assert_eq!(back.codec, "aac");
        assert_eq!(back.source, "playback");
        assert!(back.audio_dup);
        assert_eq!(back.output_device, "Speakers");
    }

    #[test]
    fn missing_fields_use_defaults() {
        let settings: Settings = serde_json::from_str(r#"{"serial":"abc"}"#).unwrap();
        assert_eq!(settings.serial, "abc");
        assert_eq!(settings.codec, "opus");
        assert_eq!(settings.buffer_ms, 50);
        assert!(settings.auto_reconnect);
    }
}
