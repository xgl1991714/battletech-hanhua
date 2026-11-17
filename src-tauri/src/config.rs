//! 应用配置（窗口位置、游戏目录、主题）的读写。
//!
//! 为兼容 Electron 版本，配置文件位置保持为：
//! `%APPDATA%\Battletech_Hanhua\config.json`

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, LogicalPosition, LogicalSize, WebviewWindow};

static CONFIG_LOCK: Mutex<()> = Mutex::new(());
static HOME_DIR: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub maximize: Option<bool>,
    pub bt_path: Option<String>,
    pub rtl_path: Option<String>,
    pub theme: Option<String>,
}

fn default_home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Battletech_Hanhua")
    }

    #[cfg(target_os = "macos")]
    {
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("Library/Preferences/Battletect_Hanhua")
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let base = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        base.join(".config/battletect_hanhua")
    }
}

fn is_writable_dir(dir: &Path) -> bool {
    if fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".write-test-{}", std::process::id()));
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

/// 数据根目录。
///
/// 默认与 Electron 版一致；支持 `BTH_HOME` 便于便携运行。若 `%APPDATA%` 不可写
/// （例如受限终端/沙箱），自动回退到 exe 同级的 `app-data/hanhua`。
pub fn home_dir() -> PathBuf {
    HOME_DIR.get_or_init(resolve_home_dir).clone()
}

fn resolve_home_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("BTH_HOME") {
        let path = PathBuf::from(value);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }

    let candidate = default_home_dir();
    if is_writable_dir(&candidate) {
        return candidate;
    }

    let fallback = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("app-data").join("hanhua")))
        .unwrap_or_else(|| PathBuf::from("app-data").join("hanhua"));
    let _ = fs::create_dir_all(&fallback);
    fallback
}

/// 旧版（Electron）默认配置路径，用于数据目录回退时迁移配置。
fn legacy_home_dir() -> PathBuf {
    default_home_dir()
}

pub fn config_path() -> PathBuf {
    home_dir().join("config.json")
}

pub fn read_config() -> AppConfig {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

/// 只读取已存在的配置；用于在创建窗口前把几何参数交给 WindowConfig。
pub fn load_existing_config() -> Option<AppConfig> {
    if config_path().is_file() {
        Some(read_config())
    } else {
        None
    }
}

/// 在窗口创建前判断保存的位置是否仍在某个显示器上。
pub fn position_is_on_monitor(app: &AppHandle, x: i32, y: i32) -> bool {
    if let Ok(monitors) = app.available_monitors() {
        for monitor in monitors {
            let scale = monitor.scale_factor();
            let position = monitor.position();
            let size = monitor.size();
            let left = position.x as f64 / scale;
            let top = position.y as f64 / scale;
            let width = size.width as f64 / scale;
            let height = size.height as f64 / scale;
            if (x as f64) >= left
                && (x as f64) <= left + width
                && (y as f64) >= top
                && (y as f64) <= top + height
            {
                return true;
            }
        }
    }
    false
}

pub fn write_config(config: &AppConfig) -> Result<(), String> {
    let _guard = CONFIG_LOCK.lock().ok();
    let home = home_dir();
    fs::create_dir_all(&home).map_err(|e| format!("创建配置目录失败: {e}"))?;
    let path = config_path();
    let text = serde_json::to_string_pretty(config).map_err(|e| format!("序列化配置失败: {e}"))?;
    fs::write(&path, text).map_err(|e| format!("写入配置失败: {e}"))?;
    Ok(())
}

/// 只更新由前端负责的字段，避免覆盖窗口状态。
pub fn patch_config(patch: &Value) -> Result<AppConfig, String> {
    let mut config = read_config();
    if let Some(value) = patch.get("btPath").and_then(Value::as_str) {
        config.bt_path = Some(value.to_string());
    }
    if let Some(value) = patch.get("rtlPath").and_then(Value::as_str) {
        config.rtl_path = Some(value.to_string());
    }
    if let Some(value) = patch.get("theme").and_then(Value::as_str) {
        config.theme = Some(value.to_string());
    }
    write_config(&config)?;
    Ok(config)
}

/// 初始化配置：不存在时自动探测游戏目录并落盘。
pub fn init_config(window: &WebviewWindow) -> AppConfig {
    let path = config_path();
    if path.is_file() {
        return read_config();
    }

    // 数据目录回退到 exe 旁时，尝试迁移原 Electron 版配置。
    let legacy_path = legacy_home_dir().join("config.json");
    if legacy_path != path && legacy_path.is_file() {
        if let Ok(text) = fs::read_to_string(&legacy_path) {
            if let Ok(config) = serde_json::from_str::<AppConfig>(&text) {
                let _ = write_config(&config);
                return config;
            }
        }
    }

    let scale = window.scale_factor().unwrap_or(1.0);
    let size = window
        .outer_size()
        .map(|s| s.to_logical::<f64>(scale))
        .unwrap_or(LogicalSize::new(1000.0, 563.0));
    let pos = window
        .outer_position()
        .map(|p| p.to_logical::<f64>(scale))
        .ok();
    let config = AppConfig {
        width: Some(size.width.round().max(1.0) as u32),
        height: Some(size.height.round().max(1.0) as u32),
        x: pos.map(|p| p.x.round() as i32),
        y: pos.map(|p| p.y.round() as i32),
        maximize: Some(false),
        bt_path: crate::game::find_battle_tech(),
        rtl_path: crate::game::find_rogue_launcher(),
        theme: Some("Light".to_string()),
    };
    let _ = write_config(&config);
    config
}

fn point_on_any_monitor(window: &WebviewWindow, x: i32, y: i32) -> bool {
    if let Ok(monitors) = window.available_monitors() {
        for monitor in monitors {
            let scale = monitor.scale_factor();
            let position = monitor.position();
            let size = monitor.size();
            let left = position.x as f64 / scale;
            let top = position.y as f64 / scale;
            let width = size.width as f64 / scale;
            let height = size.height as f64 / scale;
            if (x as f64) >= left
                && (x as f64) <= left + width
                && (y as f64) >= top
                && (y as f64) <= top + height
            {
                return true;
            }
        }
    }
    false
}

/// 应用窗口尺寸/位置/最大化状态。位置不在任何显示器上时回到屏幕中央。
/// 按外框尺寸设置窗口。`config.width/height` 现在表示包含边框/阴影的外框尺寸。
#[cfg(target_os = "windows")]
pub fn apply_outer_size(window: &WebviewWindow, width: u32, height: u32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
    };

    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let physical = LogicalSize::new(width as f64, height as f64).to_physical::<u32>(scale);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            0,
            0,
            physical.width as i32,
            physical.height as i32,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_outer_size(window: &WebviewWindow, width: u32, height: u32) {
    let _ = window.set_size(LogicalSize::new(width as f64, height as f64));
}

pub fn apply_window_config(window: &WebviewWindow, config: &AppConfig) {
    if let (Some(width), Some(height)) = (config.width, config.height) {
        if width >= 100 && height >= 100 {
            eprintln!("[window] restore outer geometry: {width}x{height}");
            apply_outer_size(window, width, height);
        }
    }

    if let (Some(x), Some(y)) = (config.x, config.y) {
        if point_on_any_monitor(window, x, y) {
            let _ = window.set_position(LogicalPosition::new(x as f64, y as f64));
        } else {
            let _ = window.center();
        }
    }

    if config.maximize.unwrap_or(false) {
        let _ = window.maximize();
    }
}

/// 关闭窗口时保存外框尺寸、位置和最大化状态。
///
/// `config.width/height` 表示外框尺寸，因此使用 `outer_size()`；
/// 最大化/最小化时保留上一次的普通窗口状态。
pub fn save_window_state(window: &tauri::Window) -> Result<(), String> {
    let maximized = window.is_maximized().unwrap_or(false);
    let minimized = window.is_minimized().unwrap_or(false);
    let mut config = read_config();

    if !maximized && !minimized {
        let scale = window.scale_factor().map_err(|e| e.to_string())?;
        let size = window
            .outer_size()
            .map_err(|e| e.to_string())?
            .to_logical::<f64>(scale);
        let position = window
            .outer_position()
            .map_err(|e| e.to_string())?
            .to_logical::<f64>(scale);
        eprintln!(
            "[window] save outer size: {:.0}x{:.0} at ({:.0},{:.0})",
            size.width, size.height, position.x, position.y
        );
        if size.width >= 100.0 && size.height >= 100.0 {
            config.width = Some(size.width.round() as u32);
            config.height = Some(size.height.round() as u32);
        }
        config.x = Some(position.x.round() as i32);
        config.y = Some(position.y.round() as i32);
    }
    config.maximize = Some(maximized);
    write_config(&config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip_keeps_legacy_keys() {
        let text = r#"{"width":1190,"height":681,"x":258,"y":196,"btPath":"E:\\Games\\BATTLETECH","rtlPath":"E:\\Games\\Roguetech","theme":"Light","maximize":false}"#;
        let config: AppConfig = serde_json::from_str(text).unwrap();
        assert_eq!(config.width, Some(1190));
        assert_eq!(config.bt_path.as_deref(), Some(r"E:\Games\BATTLETECH"));
        assert_eq!(config.theme.as_deref(), Some("Light"));
        let output = serde_json::to_string(&config).unwrap();
        assert!(output.contains("\"btPath\""));
    }
}
