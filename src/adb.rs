use crate::tools;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceState {
    Device,
    Unauthorized,
    Offline,
    NoPermissions,
    Unknown,
}

impl DeviceState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Device => "已连接",
            Self::Unauthorized => "未授权",
            Self::Offline => "离线",
            Self::NoPermissions => "无权限",
            Self::Unknown => "未知",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub serial: String,
    pub state: DeviceState,
    pub model: String,
}

/// `adb devices -l`, with a timeout so a hung adb server cannot freeze the UI.
pub fn list_devices(adb: &Path) -> Result<Vec<Device>, String> {
    let text = tools::run_timeout(adb, &["devices", "-l"], Duration::from_secs(8))?;
    Ok(parse_devices(&text))
}

pub fn parse_devices(text: &str) -> Vec<Device> {
    let mut devices = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty()
            || line.starts_with("List of devices")
            || line.starts_with('*')
            || line.starts_with("adb server")
            || line.starts_with("error:")
            || line.starts_with("warning:")
            || line.starts_with("adb:")
            || line.starts_with("failed ")
            || line.starts_with("cannot ")
        {
            continue;
        }

        let mut parts = line.split_whitespace();
        let Some(serial) = parts.next() else {
            continue;
        };
        let Some(state_text) = parts.next() else {
            continue;
        };

        let state = match state_text {
            "device" => DeviceState::Device,
            "unauthorized" => DeviceState::Unauthorized,
            "offline" => DeviceState::Offline,
            "no" => DeviceState::NoPermissions,
            _ => DeviceState::Unknown,
        };

        let mut model = String::new();
        for part in parts {
            if let Some(value) = part.strip_prefix("model:") {
                model = value.replace('_', " ");
            }
        }
        if model.is_empty() {
            model = serial.to_string();
        }

        devices.push(Device {
            serial: serial.to_string(),
            state,
            model,
        });
    }

    devices
}

pub fn pair(adb: &Path, addr: &str, code: &str) -> Result<String, String> {
    tools::run_timeout(adb, &["pair", addr, code], Duration::from_secs(30))
}

pub fn connect(adb: &Path, addr: &str) -> Result<String, String> {
    tools::run_timeout(adb, &["connect", addr], Duration::from_secs(12))
}

/// Find the `_adb-tls-connect._tcp` endpoint advertised by wireless debugging.
///
/// When `expect_ip` is set, only services on that IP are accepted, so two phones
/// on the same LAN cannot be mixed up after pairing.
pub fn find_connect_service(adb: &Path, expect_ip: Option<&str>) -> Option<String> {
    let text = tools::run_timeout(adb, &["mdns", "services"], Duration::from_secs(3)).ok()?;
    parse_mdns_services(&text, expect_ip)
}

pub fn parse_mdns_services(text: &str, expect_ip: Option<&str>) -> Option<String> {
    for line in text.lines() {
        if !line.contains("_adb-tls-connect._tcp") {
            continue;
        }
        let Some(addr) = line.split_whitespace().last() else {
            continue;
        };
        if !addr.contains(':') {
            continue;
        }
        if let Some(ip) = expect_ip {
            if !addr.starts_with(&format!("{ip}:")) && !addr.starts_with(&format!("[{ip}]:")) {
                continue;
            }
        }
        return Some(addr.to_string());
    }
    None
}

/// Host part of a TCP/IP adb serial (`192.168.1.5:5555` → `192.168.1.5`).
///
/// USB serials and mDNS service names have no `host:port` shape and return
/// `None`. This is used to recognize the same phone after its wireless
/// debugging port changes on reconnect/reboot.
pub fn serial_host(serial: &str) -> Option<&str> {
    let (host, port) = serial.rsplit_once(':')?;
    if host.is_empty() || host.contains(':') {
        return None;
    }
    if port.parse::<u16>().is_ok() {
        Some(host)
    } else {
        None
    }
}

/// Try to connect to every `_adb-tls-connect._tcp` endpoint advertised through
/// adb mDNS, and return the endpoints that were attempted.
///
/// Wireless debugging picks a new port after it is toggled/rebooted, so the
/// previously saved `ip:port` goes stale. The mDNS service name stays stable
/// and its TXT record carries the current port; re-connecting makes the phone
/// show up again without the user typing anything.
pub fn connect_mdns_services(adb: &Path) -> Vec<String> {
    let Ok(text) = tools::run_timeout(adb, &["mdns", "services"], Duration::from_secs(3)) else {
        return Vec::new();
    };

    let mut endpoints: Vec<String> = Vec::new();
    for line in text.lines() {
        if !line.contains("_adb-tls-connect._tcp") {
            continue;
        }
        let Some(addr) = line.split_whitespace().last() else {
            continue;
        };
        if serial_host(addr).is_none() {
            continue;
        }
        if endpoints.iter().any(|existing| existing == addr) {
            continue;
        }
        // `adb connect` is idempotent; errors are logged by the caller only if
        // nothing shows up in `adb devices` afterwards.
        let _ = tools::run_timeout(adb, &["connect", addr], Duration::from_secs(6));
        endpoints.push(addr.to_string());
    }
    endpoints
}

/// Read an Android system property over adb (used as a cheap pre-flight check).
pub fn shell_getprop(adb: &Path, serial: &str, key: &str) -> Result<String, String> {
    tools::run_timeout(
        adb,
        &["-s", serial, "shell", "getprop", key],
        Duration::from_secs(3),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usb_and_unauthorized_devices() {
        let text = "\
List of devices attached
R58M1234\tdevice product:beyond1 model:SM_G9730 device:beyond1 transport_id:1
192.168.1.5:5555\tunauthorized transport_id:2
";
        let devices = parse_devices(text);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].serial, "R58M1234");
        assert_eq!(devices[0].model, "SM G9730");
        assert_eq!(devices[0].state, DeviceState::Device);
        assert_eq!(devices[1].state, DeviceState::Unauthorized);
    }

    #[test]
    fn ignores_adb_noise_and_parses_no_permissions() {
        let text = "\
* daemon not running; starting now at tcp:5037
* daemon started successfully
List of devices attached
????????????\tno permissions (user in plugdev group; are your udev rules wrong?); see [http://developer.android.com/tools/device.html]
0123456789ABCDEF\toffline transport_id:3
192.168.1.5:5555\tdevice product:x model:Pixel_7 transport_id:4
adb server version (41) doesn't match this client (40); killing...
error: device 'R58M1234' not found
";
        let devices = parse_devices(text);
        assert_eq!(devices.len(), 3);
        assert_eq!(devices[0].state, DeviceState::NoPermissions);
        // No model attribute: the serial is used as a fallback label.
        assert_eq!(devices[0].model, "????????????");
        assert_eq!(devices[1].state, DeviceState::Offline);
        assert_eq!(devices[2].state, DeviceState::Device);
        assert_eq!(devices[2].model, "Pixel 7");
    }

    #[test]
    fn parses_crlf_output() {
        let text = "List of devices attached\r\nR58M1234\tdevice model:SM_G9730 transport_id:1\r\n";
        let devices = parse_devices(text);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].model, "SM G9730");
    }

    #[test]
    fn parses_serial_host() {
        assert_eq!(serial_host("192.168.1.5:5555"), Some("192.168.1.5"));
        assert_eq!(serial_host("192.168.1.100:42359"), Some("192.168.1.100"));
        assert_eq!(serial_host("R58M1234"), None);
        assert_eq!(
            serial_host("adb-0123456789ABCDEF-example._adb-tls-connect._tcp"),
            None
        );
        assert_eq!(serial_host("host:notaport"), None);
        assert_eq!(serial_host(""), None);
    }

    #[test]
    fn finds_mdns_connect_service() {
        let text = "\
List of discovered mdns services
adb-R58M1234-xyz\t_adb-tls-connect._tcp\t192.168.1.5:37123
adb-OTHER-abc\t_adb-tls-connect._tcp\t192.168.1.9:40000
";
        assert_eq!(
            parse_mdns_services(text, Some("192.168.1.9")),
            Some("192.168.1.9:40000".to_string())
        );
        assert_eq!(
            parse_mdns_services(text, None),
            Some("192.168.1.5:37123".to_string())
        );
        assert_eq!(parse_mdns_services(text, Some("192.168.1.99")), None);
        assert_eq!(parse_mdns_services("nothing here", None), None);
    }
}
