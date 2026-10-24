use crate::settings::Settings;
use crate::tools;
use std::time::Duration;

/// Everything we know about the bundled/`PATH` scrcpy build.
///
/// Capabilities are derived from `--version` + `--help` output rather than
/// hard-coded versions, so a patched build (with `--audio-output-device`) and
/// future upstream versions are detected automatically.
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub version_line: String,
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub help: String,
    pub available: bool,
    pub error: Option<String>,
}

impl Info {
    pub fn detect() -> Self {
        let scrcpy = tools::find_scrcpy();
        let mut info = Self::default();

        match tools::run_timeout(&scrcpy, &["--version"], Duration::from_secs(5)) {
            Ok(text) => {
                info.available = true;
                info.version_line = first_line(&text);
                if let Some((major, minor, patch)) = parse_version(&info.version_line) {
                    info.major = major;
                    info.minor = minor;
                    info.patch = patch;
                }
                match tools::run_timeout(&scrcpy, &["--help"], Duration::from_secs(5)) {
                    Ok(help) => info.help = help,
                    Err(error) => info.error = Some(format!("无法读取 scrcpy --help：{error}")),
                }
            }
            Err(error) => {
                info.error = Some(first_line(&error));
            }
        }

        info
    }

    /// Returns true when `--<long>` is listed as an option in `--help`.
    pub fn supports(&self, long: &str) -> bool {
        let needle = format!("--{long}");
        for line in self.help.lines() {
            let mut tokens = line.split_whitespace();
            // Only inspect the option column ("-s, --serial=serial"), never
            // the description text.
            for _ in 0..8 {
                let Some(token) = tokens.next() else { break };
                let token = token.trim_end_matches(',');
                if token == needle
                    || token.starts_with(&format!("{needle}="))
                    || token.starts_with(&format!("{needle}["))
                {
                    return true;
                }
                if !token.starts_with('-') {
                    break;
                }
            }
        }
        false
    }

    pub fn at_least(&self, major: u32, minor: u32) -> bool {
        (self.major, self.minor) >= (major, minor)
    }

    /// `--no-video` and `--audio-source` were added in scrcpy 2.1.
    pub fn audio_ok(&self) -> bool {
        self.available && self.supports("no-video") && self.supports("audio-source")
    }

    /// Playback capture + `--audio-dup` were added in scrcpy 2.6.
    pub fn playback_ok(&self) -> bool {
        self.audio_ok() && self.supports("audio-dup")
    }

    /// FLAC audio encoding was added in scrcpy 2.3.
    pub fn flac_ok(&self) -> bool {
        self.audio_ok() && self.at_least(2, 3)
    }

    /// Only a patched build (this project ships one) advertises this option.
    pub fn output_device_ok(&self) -> bool {
        self.available && self.supports("audio-output-device")
    }

    pub fn list_audio_devices_ok(&self) -> bool {
        self.available && self.supports("list-audio-devices")
    }

    /// Enumerate the computer's audio output devices using the patched scrcpy.
    pub fn list_audio_devices(&self) -> Result<Vec<String>, String> {
        if !self.list_audio_devices_ok() {
            return Err(
                "当前 scrcpy 不支持 --list-audio-devices（需要本项目补丁版 scrcpy）".into(),
            );
        }
        let text = tools::run_timeout(
            &tools::find_scrcpy(),
            &["--list-audio-devices"],
            Duration::from_secs(12),
        )?;
        let devices: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            // scrcpy prints its version banner on stdout for every command.
            .filter(|line| !line.starts_with("scrcpy ") && !line.contains("<https://"))
            .map(str::to_string)
            .collect();
        if devices.is_empty() {
            Err("scrcpy 没有枚举到任何音频输出设备".into())
        } else {
            Ok(devices)
        }
    }

    pub fn summary(&self) -> String {
        if self.available {
            format!(
                "scrcpy {}.{}.{}（输出设备选择：{}）",
                self.major,
                self.minor,
                self.patch,
                if self.output_device_ok() {
                    "支持"
                } else {
                    "不支持"
                }
            )
        } else {
            "未找到 scrcpy".to_string()
        }
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

pub fn parse_version(line: &str) -> Option<(u32, u32, u32)> {
    let rest = line.trim().strip_prefix("scrcpy ")?;
    let number = rest.split_whitespace().next()?;
    let mut parts = number.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

/// Build the scrcpy command line for pure-audio streaming.
pub fn build_args(settings: &Settings, info: &Info) -> Result<Vec<String>, String> {
    if settings.serial.trim().is_empty() {
        return Err("没有选择设备".to_string());
    }
    if !info.available {
        return Err(info
            .error
            .clone()
            .unwrap_or_else(|| "未找到 scrcpy".to_string()));
    }
    if info.help.trim().is_empty() {
        return Err(info
            .error
            .clone()
            .unwrap_or_else(|| "无法读取 scrcpy --help，无法确认版本能力".to_string()));
    }
    if !info.audio_ok() {
        return Err(format!(
            "scrcpy {} 过旧：纯音频串流需要 scrcpy >= 2.1（--no-video / --audio-source）",
            info.version_line
        ));
    }

    let codec = match settings.codec.as_str() {
        "aac" => "aac",
        "raw" => "raw",
        "flac" => {
            if !info.flac_ok() {
                return Err("FLAC 音频编码需要 scrcpy >= 2.3".to_string());
            }
            "flac"
        }
        _ => "opus",
    };

    let source = if settings.source == "playback" {
        if !info.playback_ok() {
            return Err(
                "「播放捕获 / 保留手机外放」需要 scrcpy >= 2.6，且手机为 Android 13+".to_string(),
            );
        }
        "playback"
    } else {
        "output"
    };

    let buffer_ms = settings.buffer_ms.clamp(20, 500);

    let mut args: Vec<String> = vec![
        "--serial".to_string(),
        settings.serial.trim().to_string(),
        "--no-video".to_string(),
        "--no-control".to_string(),
        "--no-window".to_string(),
    ];

    // Fail loudly instead of showing a green "streaming" state with no sound.
    if info.supports("require-audio") {
        args.push("--require-audio".to_string());
    }

    args.push("--audio-source".to_string());
    args.push(source.to_string());
    args.push("--audio-codec".to_string());
    args.push(codec.to_string());
    args.push("--audio-buffer".to_string());
    args.push(buffer_ms.to_string());

    // `--audio-bit-rate` does not apply to raw PCM.
    if codec != "raw" {
        let kbps = settings.bitrate_kbps.clamp(32, 512);
        args.push("--audio-bit-rate".to_string());
        args.push(format!("{kbps}K"));
    }

    if source == "playback" && settings.audio_dup {
        args.push("--audio-dup".to_string());
    }

    if info.output_device_ok() && !settings.output_device.trim().is_empty() {
        args.push("--audio-output-device".to_string());
        args.push(settings.output_device.trim().to_string());
    }

    Ok(args)
}

/// Extra environment variables for the scrcpy child process.
///
/// On Linux, builds without the output-device patch can still be steered by
/// forcing SDL's PulseAudio driver and selecting the sink with `PULSE_SINK`.
#[cfg(target_os = "linux")]
pub fn child_env(settings: &Settings, info: &Info) -> Vec<(String, String)> {
    let mut env = Vec::new();
    let device = settings.output_device.trim();
    if !device.is_empty() && !info.output_device_ok() {
        env.push(("PULSE_SINK".to_string(), device.to_string()));
        env.push(("SDL_AUDIODRIVER".to_string(), "pulseaudio".to_string()));
    }
    env
}

#[cfg(not(target_os = "linux"))]
pub fn child_env(_settings: &Settings, _info: &Info) -> Vec<(String, String)> {
    Vec::new()
}

/// List selectable output devices.
///
/// Preference order: patched scrcpy enumeration, then PulseAudio/PipeWire
/// sinks on Linux. On Windows an unpatched scrcpy cannot be steered at all.
pub fn list_audio_output_devices(info: &Info) -> Result<Vec<String>, String> {
    if info.list_audio_devices_ok() {
        return info.list_audio_devices();
    }

    #[cfg(target_os = "linux")]
    {
        list_pulse_sinks()
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(
            "当前 scrcpy 不支持指定输出设备（需要本项目补丁版 scrcpy）；\
             也可以先把有线耳机设为系统默认输出"
                .to_string(),
        )
    }
}

/// Parse `pactl list short sinks` output: `index<TAB>name<TAB>driver...`.
#[cfg(target_os = "linux")]
fn list_pulse_sinks() -> Result<Vec<String>, String> {
    let text = tools::run_timeout(
        std::path::Path::new("pactl"),
        &["list", "short", "sinks"],
        Duration::from_secs(5),
    )
    .map_err(|error| format!("无法枚举 PulseAudio/PipeWire 输出设备：{error}"))?;
    let sinks: Vec<String> = text
        .lines()
        .filter_map(|line| {
            line.split('\t')
                .nth(1)
                .or_else(|| line.split_whitespace().nth(1))
        })
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect();
    if sinks.is_empty() {
        Err("pactl 没有列出任何输出设备（未安装 pactl 或音频服务不可用）".to_string())
    } else {
        Ok(sinks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_HELP: &str = "\
Usage: scrcpy [options]

Options:

    --audio-bit-rate=value
        Encode the audio at the given bit rate.

    --audio-buffer=ms
        Configure the audio buffering delay.

    --audio-codec=name
        Select an audio codec (opus, aac, flac or raw).

    --audio-dup
        Duplicate audio.

    --audio-output-buffer=ms
        Configure the size of the SDL audio output buffer.

    --audio-source=source
        Select the audio source.

    -n, --no-control
        Disable device control.

    --no-video
        Disable video forwarding.

    --no-window
        Disable scrcpy window. Implies --no-video-playback.

    --require-audio
        Make scrcpy fail if audio capture fails.

    -s, --serial=serial
        The device serial number.
";

    fn sample_info() -> Info {
        Info {
            version_line: "scrcpy 5.0.1 <https://github.com/Genymobile/scrcpy>".to_string(),
            major: 5,
            minor: 0,
            patch: 1,
            help: SAMPLE_HELP.to_string(),
            available: true,
            error: None,
        }
    }

    #[test]
    fn parses_versions() {
        assert_eq!(
            parse_version("scrcpy 5.0.1 <https://github.com/Genymobile/scrcpy>"),
            Some((5, 0, 1))
        );
        assert_eq!(parse_version("scrcpy 2.1"), Some((2, 1, 0)));
        assert_eq!(parse_version("scrcpy 1.25"), Some((1, 25, 0)));
        assert_eq!(parse_version("unexpected output"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn detects_options_without_false_positives() {
        let info = sample_info();
        assert!(info.supports("audio-source"));
        assert!(info.supports("audio-buffer"));
        assert!(info.supports("audio-codec"));
        assert!(info.supports("no-video"));
        assert!(info.supports("no-window"));
        assert!(info.supports("no-control"));
        assert!(info.supports("require-audio"));
        // Descriptions must not be matched as options.
        assert!(!info.supports("video"));
        // `--audio-output-buffer` must not be mistaken for `--audio-output-device`.
        assert!(!info.supports("audio-output-device"));
        assert!(!info.supports("list-audio-devices"));
    }

    #[test]
    fn builds_minimal_audio_args() {
        let info = sample_info();
        let settings = Settings::default();
        let mut settings = Settings {
            serial: "R58M1234".to_string(),
            ..settings
        };
        settings.normalize();
        let args = build_args(&settings, &info).unwrap();
        assert!(args.windows(2).any(|w| w == ["--serial", "R58M1234"]));
        for flag in [
            "--no-video",
            "--no-control",
            "--no-window",
            "--require-audio",
        ] {
            assert!(args.iter().any(|arg| arg == flag), "missing {flag}");
        }
        assert!(args.windows(2).any(|w| w == ["--audio-source", "output"]));
        assert!(args.windows(2).any(|w| w == ["--audio-codec", "opus"]));
        assert!(args.windows(2).any(|w| w == ["--audio-buffer", "50"]));
        assert!(args.windows(2).any(|w| w == ["--audio-bit-rate", "128K"]));
        assert!(!args.iter().any(|arg| arg == "--audio-dup"));
        assert!(!args.iter().any(|arg| arg == "--audio-output-device"));
    }

    #[test]
    fn raw_codec_drops_bitrate() {
        let info = sample_info();
        let settings = Settings {
            serial: "x".to_string(),
            codec: "raw".to_string(),
            ..Default::default()
        };
        let args = build_args(&settings, &info).unwrap();
        assert!(!args.iter().any(|arg| arg == "--audio-bit-rate"));
        assert!(args.windows(2).any(|w| w == ["--audio-codec", "raw"]));
    }

    #[test]
    fn output_device_used_only_when_supported() {
        let mut info = sample_info();
        let mut settings = Settings {
            serial: "x".to_string(),
            output_device: "Speakers (Realtek)".to_string(),
            ..Default::default()
        };
        settings.normalize();
        let args = build_args(&settings, &info).unwrap();
        assert!(!args.iter().any(|arg| arg == "--audio-output-device"));

        info.help
            .push_str("\n    --audio-output-device=name\n        Select output device.\n");
        assert!(info.supports("audio-output-device"));
        let args = build_args(&settings, &info).unwrap();
        assert!(args
            .windows(2)
            .any(|w| w == ["--audio-output-device", "Speakers (Realtek)"]));
    }

    #[test]
    fn rejects_old_scrcpy_and_playback_on_old_build() {
        let settings = Settings {
            serial: "x".to_string(),
            ..Default::default()
        };

        let mut old = sample_info();
        old.help = old.help.replace("--no-video", "--old-no-video");
        let error = build_args(&settings, &old).unwrap_err();
        assert!(error.contains(">= 2.1"), "unexpected error: {error}");

        let mut no_playback = sample_info();
        no_playback.help = no_playback.help.replace("--audio-dup", "--x-audio-dup");
        let mut playback = settings.clone();
        playback.source = "playback".to_string();
        let error = build_args(&playback, &no_playback).unwrap_err();
        assert!(error.contains(">= 2.6"), "unexpected error: {error}");
    }
}
