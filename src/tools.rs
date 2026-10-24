use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Build a `Command` that does not flash a console window on Windows.
pub fn command(program: &Path) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = Command::new(program);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    }
    #[cfg(not(windows))]
    {
        Command::new(program)
    }
}

/// Directories searched for bundled `scrcpy`, `adb` and `scrcpy-server`.
///
/// Order:
/// 1. `PHONE_AUDIO_BIN`
/// 2. `<exe dir>/bin`
/// 3. `<exe dir>` (portable layout)
/// 4. `<cargo manifest>/bin` (development builds only)
pub fn bin_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PHONE_AUDIO_BIN") {
        let path = PathBuf::from(dir);
        if path.exists() {
            return Some(path);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let bundled = dir.join("bin");
            if bundled.exists() {
                return Some(bundled);
            }
            let scrcpy = dir.join(format!("scrcpy{}", std::env::consts::EXE_SUFFIX));
            if scrcpy.exists() {
                return Some(dir.to_path_buf());
            }
        }
    }

    // Development convenience: when running from the project tree the bundled
    // binaries live in the manifest's `bin/`. Release packages always ship a
    // `bin/` next to the executable, which is checked above.
    if let Some(dir) = option_env!("CARGO_MANIFEST_DIR") {
        let path = Path::new(dir).join("bin");
        if path.exists() {
            return Some(path);
        }
    }

    None
}

pub fn find_tool(name: &str) -> PathBuf {
    let file_name = format!("{name}{}", std::env::consts::EXE_SUFFIX);

    if let Some(dir) = bin_dir() {
        for candidate in [dir.join(&file_name), dir.join("scrcpy").join(&file_name)] {
            if candidate.exists() {
                return candidate;
            }
        }
    }

    // Fall back to PATH.
    PathBuf::from(if cfg!(windows) {
        file_name
    } else {
        name.to_string()
    })
}

pub fn find_scrcpy() -> PathBuf {
    find_tool("scrcpy")
}

pub fn find_adb() -> PathBuf {
    find_tool("adb")
}

/// Locate the scrcpy server binary shipped next to the client.
///
/// When no bundled server is found, `None` is returned and scrcpy locates its
/// own server (system install), which is the correct behaviour.
pub fn find_server() -> Option<PathBuf> {
    let dir = bin_dir()?;
    let candidates = [
        dir.join("scrcpy-server"),
        dir.join("scrcpy-server.jar"),
        dir.join("scrcpy").join("scrcpy-server"),
        dir.join("share").join("scrcpy").join("scrcpy-server"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

/// Run a command, returning combined stdout+stderr.
///
/// The child is killed if it does not finish within `timeout`, so a stuck
/// `adb` can never block the engine thread forever.
pub fn run_timeout(program: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = command(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("无法执行 {}：{err}", program.display()))?;

    let stdout = child.stdout.take().map(|pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::BufReader::new(pipe).read_to_end(&mut buf);
            buf
        })
    });
    let stderr = child.stderr.take().map(|pipe| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::BufReader::new(pipe).read_to_end(&mut buf);
            buf
        })
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} 超时（{} 秒）",
                    program.display(),
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("等待 {} 失败：{err}", program.display()));
            }
        }
    };

    let stdout = stdout.map(|handle| handle.join().unwrap_or_default());
    let stderr = stderr.map(|handle| handle.join().unwrap_or_default());
    let mut text = String::new();
    if let Some(stdout) = stdout {
        text.push_str(&String::from_utf8_lossy(&stdout));
    }
    if let Some(stderr) = stderr {
        text.push_str(&String::from_utf8_lossy(&stderr));
    }

    if status.success() {
        Ok(text.trim().to_string())
    } else {
        Err(text.trim().to_string())
    }
}

/// Terminate a scrcpy child process, including its process tree on Windows.
///
/// Windows does not kill children when the parent exits, so a hard `kill()` of
/// `scrcpy.exe` can leave audio playing; `taskkill /T` takes the tree down.
pub fn stop_child(child: &mut Child) {
    #[cfg(windows)]
    {
        let pid = child.id().to_string();
        let _ = command(Path::new("taskkill"))
            .args(["/PID", &pid, "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[cfg(unix)]
    {
        let pid = child.id().to_string();
        // SIGTERM lets SDL/scrcpy run its cleanup path (remove server, restore
        // device state) before we fall back to SIGKILL.
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(Duration::from_millis(300));
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &pid])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let _ = child.kill();
    let _ = child.wait();
}
