//! 系统信息与机器码（与 Electron 版算法保持一致：MachineGuid 的 MD5）。

use md5::{Digest, Md5};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub product_name: String,
    pub tauri: String,
    pub webview: String,
    pub os: String,
    pub machine_id: String,
    pub arch: String,
}

pub const PRODUCT_NAME: &str = "BATTLETECH汉化工具";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const API_HOST: &str = "battletech.natapp1.cc";
pub const API_PORT: u16 = 80;

#[cfg(target_os = "windows")]
fn windows_machine_guid() -> Option<String> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Cryptography",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .ok()?;
    let guid: String = key.get_value("MachineGuid").ok()?;
    let guid = guid.trim().to_string();
    if guid.is_empty() {
        None
    } else {
        Some(guid)
    }
}

/// 返回 MachineGuid 的 MD5（小写十六进制），失败时返回空串。
pub fn machine_id() -> String {
    #[cfg(target_os = "windows")]
    let raw = windows_machine_guid();
    #[cfg(target_os = "linux")]
    let raw = std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|text| text.trim().to_string());
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let raw: Option<String> = None;

    match raw {
        Some(value) => {
            let digest = Md5::digest(value.as_bytes());
            hex::encode(digest)
        }
        None => String::new(),
    }
}

#[cfg(target_os = "windows")]
fn windows_os_description() -> String {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .ok();
    let Some(key) = key else {
        return format!("Windows ({})", std::env::consts::ARCH);
    };
    let product: String = key.get_value("ProductName").unwrap_or_default();
    let build: String = key.get_value("CurrentBuild").unwrap_or_default();
    let ubr: u32 = key.get_value("UBR").unwrap_or(0);
    let display: String = key.get_value("DisplayVersion").unwrap_or_default();
    let version = if ubr > 0 {
        format!("10.0.{build}.{ubr}")
    } else if !display.is_empty() {
        display
    } else {
        build
    };
    format!(
        "{} {} ({})",
        product.trim(),
        version.trim(),
        std::env::consts::ARCH
    )
}

#[cfg(not(target_os = "windows"))]
fn windows_os_description() -> String {
    format!(
        "{} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

pub fn app_info() -> AppInfo {
    let webview = tauri::webview_version().unwrap_or_else(|_| "unknown".to_string());
    AppInfo {
        version: APP_VERSION.to_string(),
        product_name: PRODUCT_NAME.to_string(),
        tauri: tauri::VERSION.to_string(),
        webview,
        os: windows_os_description(),
        machine_id: machine_id(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_id_is_md5_when_available() {
        let id = machine_id();
        assert!(id.is_empty() || (id.len() == 32 && id.chars().all(|ch| ch.is_ascii_hexdigit())));
    }
}
