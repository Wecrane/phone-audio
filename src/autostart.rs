//! Per-user "start on login" integration.
//!
//! Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run (no admin needed).
//! Linux: ~/.config/autostart/phone-audio.desktop.
//! Other platforms: unsupported (the UI reports the error).

#[cfg(windows)]
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const VALUE_NAME: &str = "PhoneAudio";

/// Is the app currently registered to start on login?
pub fn is_enabled() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let status = crate::tools::command(std::path::Path::new("reg"))
            .args(["query", RUN_KEY, "/v", VALUE_NAME])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|error| format!("无法查询注册表：{error}"))?;
        Ok(status.success())
    }

    #[cfg(target_os = "linux")]
    {
        Ok(desktop_file().is_file())
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        Ok(false)
    }
}

/// Enable or disable start-on-login. Returns a user-facing error message on failure.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        set_windows(enabled)
    }

    #[cfg(target_os = "linux")]
    {
        set_linux(enabled)
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = enabled;
        Err("当前系统暂不支持开机自启动".to_string())
    }
}

#[cfg(windows)]
fn set_windows(enabled: bool) -> Result<(), String> {
    use std::process::Stdio;

    let mut command = crate::tools::command(std::path::Path::new("reg"));
    if enabled {
        let exe = std::env::current_exe().map_err(|error| format!("无法获取程序路径：{error}"))?;
        let command_line = format!("\"{}\"", exe.display());
        command.args([
            "add",
            RUN_KEY,
            "/v",
            VALUE_NAME,
            "/t",
            "REG_SZ",
            "/d",
            &command_line,
            "/f",
        ]);
    } else {
        command.args(["delete", RUN_KEY, "/v", VALUE_NAME, "/f"]);
    }

    let output = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("无法运行 reg.exe：{error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if message.is_empty() {
            "注册表写入失败".to_string()
        } else {
            message
        })
    }
}

#[cfg(target_os = "linux")]
fn desktop_file() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".config"));
    base.join("autostart").join("phone-audio.desktop")
}

#[cfg(target_os = "linux")]
fn set_linux(enabled: bool) -> Result<(), String> {
    let path = desktop_file();
    if !enabled {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("无法删除 {}：{error}", path.display())),
        };
    }

    let exe = std::env::current_exe().map_err(|error| format!("无法获取程序路径：{error}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("无法创建 {}：{error}", parent.display()))?;
    }
    let exec = exe.display().to_string().replace('"', "\\\"");
    let contents = format!(
        "[Desktop Entry]\nType=Application\nName=Phone Audio\nComment=手机音频串流到电脑\nExec=\"{exec}\"\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    );
    std::fs::write(&path, contents).map_err(|error| format!("无法写入 {}：{error}", path.display()))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn desktop_path_lives_under_autostart() {
        let path = desktop_file();
        assert!(path.ends_with("autostart/phone-audio.desktop"));
    }
}
