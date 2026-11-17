//! Tauri 命令层：前端通过 `invoke` 调用的所有后端能力。

use crate::localization::{
    ApplyRequest, BackupRequest, BusyGuard, DeleteBackupRequest, RestoreRequest,
};
use crate::{config, data_query, game, localization, system, update, AppState};
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

#[tauri::command]
pub fn get_app_info() -> system::AppInfo {
    system::app_info()
}

#[tauri::command]
pub fn get_machine_id() -> String {
    system::machine_id()
}

#[tauri::command]
pub fn get_config() -> config::AppConfig {
    config::read_config()
}

#[tauri::command]
pub fn update_config(patch: Value) -> Result<config::AppConfig, String> {
    config::patch_config(&patch)
}

#[tauri::command(rename_all = "camelCase")]
pub fn scan_game(root_path: String, rtl_path: String) -> game::GameStatus {
    game::scan_game(&root_path, &rtl_path)
}

#[tauri::command(rename_all = "camelCase")]
pub fn reset_game_path(folder: String) -> Result<game::GameStatus, String> {
    let path = PathBuf::from(&folder);
    if !path.is_dir() {
        return Err("所选目录不存在。".to_string());
    }
    config::patch_config(&json!({ "btPath": folder.clone() }))?;
    let config = config::read_config();
    Ok(game::scan_game(
        &folder,
        config.rtl_path.as_deref().unwrap_or_default(),
    ))
}

#[tauri::command(rename_all = "camelCase")]
pub fn set_rtl_path(folder: String) -> Result<game::GameStatus, String> {
    let path = PathBuf::from(&folder);
    if !path.is_dir() {
        return Err("所选目录不存在。".to_string());
    }
    config::patch_config(&json!({ "rtlPath": folder.clone() }))?;
    let config = config::read_config();
    Ok(game::scan_game(
        config.bt_path.as_deref().unwrap_or_default(),
        &folder,
    ))
}

#[tauri::command(rename_all = "camelCase")]
pub fn get_data_info(is_rt: bool) -> localization::DataInfo {
    localization::data_info(is_rt)
}

#[tauri::command(rename_all = "camelCase")]
pub fn list_backups(
    root_path: String,
    is_rt: bool,
    mia_version: String,
) -> Vec<localization::BackupEntry> {
    localization::list_backups(&root_path, is_rt, &mia_version)
}

#[tauri::command]
pub async fn get_latest_version() -> Result<String, String> {
    localization::latest_version().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn install_localization(
    app: AppHandle,
    state: State<'_, AppState>,
    machine_id: String,
    app_version: String,
    expected: String,
) -> Result<String, String> {
    let busy = state.busy.clone();
    localization::install_localization(app, busy, machine_id, app_version, expected).await
}

#[tauri::command]
pub async fn apply_localization(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ApplyRequest,
) -> Result<localization::TaskSummary, String> {
    let busy = state.busy.clone();
    let _guard = BusyGuard::acquire(&busy)?;
    let app_for_task = app.clone();
    tokio::task::spawn_blocking(move || {
        localization::apply_localization(&app_for_task, &request)
    })
    .await
    .map_err(|e| format!("汉化任务执行失败: {e}"))?
}

#[tauri::command]
pub async fn backup_localization(
    app: AppHandle,
    state: State<'_, AppState>,
    request: BackupRequest,
) -> Result<localization::TaskSummary, String> {
    let busy = state.busy.clone();
    let _guard = BusyGuard::acquire(&busy)?;
    let app_for_task = app.clone();
    tokio::task::spawn_blocking(move || {
        localization::backup_localization(&app_for_task, &request)
    })
    .await
    .map_err(|e| format!("备份任务执行失败: {e}"))?
}

#[tauri::command]
pub async fn restore_localization(
    app: AppHandle,
    state: State<'_, AppState>,
    request: RestoreRequest,
) -> Result<localization::TaskSummary, String> {
    let busy = state.busy.clone();
    let _guard = BusyGuard::acquire(&busy)?;
    let app_for_task = app.clone();
    tokio::task::spawn_blocking(move || {
        localization::restore_localization(&app_for_task, &request)
    })
    .await
    .map_err(|e| format!("恢复任务执行失败: {e}"))?
}

#[tauri::command]
pub fn delete_backup(request: DeleteBackupRequest) -> Result<(), String> {
    localization::delete_backup(&request)
}

#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pending = state.pending_update.clone();
    update::check_update(app, pending).await
}

#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let pending = state.pending_update.clone();
    update::install_update(app, pending).await
}

/// 前端首帧渲染完成后显示窗口，避免白屏。
#[tauri::command]
pub fn show_main(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub fn hide_main(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn window_is_maximized(app: AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_maximized().ok())
        .unwrap_or(false)
}

/// 同步原生标题栏的浅色/深色主题。
#[tauri::command]
pub fn set_window_theme(app: AppHandle, theme: String) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    let theme = if theme.eq_ignore_ascii_case("dark") {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    };
    window.set_theme(Some(theme)).map_err(|e| e.to_string())
}

/// 扫描 `Mods` 目录并返回可查询的数据索引。
#[tauri::command]
pub async fn data_query_entries(
    root: String,
    state: State<'_, AppState>,
) -> Result<Vec<data_query::DataEntry>, String> {
    let cache = state.data_cache.clone();
    if let Some(existing) = cache.lock().map_err(|e| e.to_string())?.as_ref() {
        if existing.root.eq_ignore_ascii_case(&root) {
            return Ok(existing.entries.clone());
        }
    }

    let scan_root = root.clone();
    let built = tauri::async_runtime::spawn_blocking(move || data_query::scan_or_load(&scan_root))
        .await
        .map_err(|e| format!("扫描任务失败: {e}"))??;
    let entries = built.entries.clone();
    *cache.lock().map_err(|e| e.to_string())? = Some(built);
    Ok(entries)
}

/// 查询某个机甲/载具/武器/装备的详细信息。
#[tauri::command]
pub fn data_query_detail(
    root: String,
    kind: String,
    id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let guard = state.data_cache.lock().map_err(|e| e.to_string())?;
    let cache = guard
        .as_ref()
        .ok_or_else(|| "数据索引尚未加载，请先打开数据查询。".to_string())?;
    if !cache.root.eq_ignore_ascii_case(&root) {
        return Err("游戏目录已变化，请重新打开数据查询。".to_string());
    }
    data_query::detail(cache, &kind, &id)
}

/// 供调试/自动化使用：返回配置路径与数据目录。
#[tauri::command]
pub async fn data_query_resolve_icons(
    icons: Vec<String>,
    state: State<'_, AppState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let root = {
        let guard = state.data_cache.lock().map_err(|e| e.to_string())?;
        guard
            .as_ref()
            .ok_or_else(|| "数据索引尚未加载，请先打开数据查询。".to_string())?
            .root
            .clone()
    };
    tauri::async_runtime::spawn_blocking(move || data_query::resolve_builtin_icons(&root, &icons))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_paths() -> Value {
    json!({
        "home": config::home_dir().to_string_lossy(),
        "config": config::config_path().to_string_lossy(),
        "data": localization::data_root().to_string_lossy()
    })
}
