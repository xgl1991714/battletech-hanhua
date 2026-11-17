//! BATTLETECH 汉化工具 —— Tauri 2 + Rust 后端入口。

mod commands;
mod config;
pub mod data_query;
mod game;
mod localization;
mod sevenzip;
mod system;
mod update;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Manager;

pub struct AppState {
    pub busy: Arc<AtomicBool>,
    pub pending_update: Arc<Mutex<Option<update::PendingUpdate>>>,
    pub data_cache: Arc<Mutex<Option<data_query::DataCache>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            busy: Arc::new(AtomicBool::new(false)),
            pending_update: Arc::new(Mutex::new(None)),
            data_cache: Arc::new(Mutex::new(None)),
        }
    }
}

fn setup_error(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, message.into())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            // 不使用 Tauri 的自动窗口创建，以便把 WebView2 数据目录放到可写位置。
            // 默认的 %LOCALAPPDATA%\<identifier> 在某些受限环境会返回 os error 5。
            let mut window_config = app
                .config()
                .app
                .windows
                .first()
                .cloned()
                .ok_or_else(|| setup_error("缺少主窗口配置"))?;

            // 有已保存配置时：位置交给窗口创建，尺寸在建窗后按“外框”还原。
            let saved_config = config::load_existing_config();
            if let Some(saved) = &saved_config {
                if let (Some(x), Some(y)) = (saved.x, saved.y) {
                    if config::position_is_on_monitor(app.handle(), x, y) {
                        window_config.x = Some(x as f64);
                        window_config.y = Some(y as f64);
                    } else {
                        window_config.x = None;
                        window_config.y = None;
                        window_config.center = true;
                    }
                }
                // 先按普通窗口创建，应用外框尺寸后再最大化，保证还原时的普通边界正确。
                window_config.maximized = false;
            }

            let webview_data_dir = config::home_dir().join("webview");
            std::fs::create_dir_all(&webview_data_dir).map_err(|e| {
                setup_error(format!(
                    "创建 WebView2 数据目录失败 {}: {e}",
                    webview_data_dir.display()
                ))
            })?;
            let webview_data_dir =
                std::path::absolute(&webview_data_dir).unwrap_or_else(|_| webview_data_dir.clone());

            let window = tauri::WebviewWindowBuilder::from_config(app.handle(), &window_config)
                .map_err(|e| setup_error(format!("读取主窗口配置失败: {e}")))?
                .data_directory(webview_data_dir)
                .build()
                .map_err(|e| setup_error(format!("创建主窗口失败: {e}")))?;

            if let Some(saved) = &saved_config {
                if let (Some(width), Some(height)) = (saved.width, saved.height) {
                    if width >= 100 && height >= 100 {
                        config::apply_outer_size(&window, width, height);
                    }
                }
                if saved.maximize.unwrap_or(false) {
                    let _ = window.maximize();
                }
                eprintln!(
                    "[window] build from config (outer): {:?}x{:?} at ({:?},{:?}), maximized={}",
                    saved.width,
                    saved.height,
                    saved.x,
                    saved.y,
                    saved.maximize.unwrap_or(false)
                );
            } else {
                // 首次运行/旧配置迁移时才需要在建窗后应用一次几何参数。
                let app_config = config::init_config(&window);
                config::apply_window_config(&window, &app_config);
            }

            // 兜底：前端异常时 3 秒后仍显示窗口，避免“进程在但看不见”。
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(3));
                if let Some(window) = handle.get_webview_window("main") {
                    if !window.is_visible().unwrap_or(false) {
                        let _ = window.show();
                    }
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if window.label() == "main" {
                    let _ = config::save_window_state(window);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_machine_id,
            commands::get_config,
            commands::update_config,
            commands::scan_game,
            commands::reset_game_path,
            commands::set_rtl_path,
            commands::get_data_info,
            commands::list_backups,
            commands::get_latest_version,
            commands::install_localization,
            commands::apply_localization,
            commands::backup_localization,
            commands::restore_localization,
            commands::delete_backup,
            commands::check_update,
            commands::install_update,
            commands::show_main,
            commands::hide_main,
            commands::window_is_maximized,
            commands::set_window_theme,
            commands::data_query_entries,
            commands::data_query_detail,
            commands::data_query_resolve_icons,
            commands::get_paths,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用失败");
}
