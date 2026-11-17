//! 汉化内容下载、解压、替换、备份、恢复与 RT 配置处理。
//!
//! 数据目录（与 Electron 版不同，改为可写的用户数据目录，避免安装目录权限问题）：
//! `%APPDATA%\Battletech_Hanhua\data`、`data_bak`、`data_tmp.7z`。
//! 可通过 `BTH_HOME` 覆盖根目录。

use crate::{config, sevenzip};
use futures_util::StreamExt;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use walkdir::WalkDir;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub id: usize,
    pub name: String,
    pub path: String,
    pub is_folder: bool,
    pub level: u32,
    pub size: Option<u64>,
    pub status: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataInfo {
    pub version: String,
    pub files: Vec<FileNode>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub done: u32,
    pub failed: u32,
    pub total: u32,
    pub time: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub root_path: String,
    pub rtl_path: String,
    pub is_bt: bool,
    pub is_rt: bool,
    pub platform: String,
    pub game_version: String,
    pub mia_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupRequest {
    pub root_path: String,
    pub is_rt: bool,
    pub mia_version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreRequest {
    pub root_path: String,
    pub is_rt: bool,
    pub mia_version: String,
    pub time: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBackupRequest {
    pub root_path: String,
    pub is_rt: bool,
    pub mia_version: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    phase: String,
    percent: u32,
    completed: u32,
    total: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileStatusEvent {
    path: String,
    status: String,
    error: String,
}

/// 防止下载/汉化/备份/恢复同时运行。
pub struct BusyGuard {
    flag: Arc<AtomicBool>,
}

impl BusyGuard {
    pub fn acquire(flag: &Arc<AtomicBool>) -> Result<Self, String> {
        if flag.swap(true, Ordering::SeqCst) {
            Err("当前已有任务正在进行，请等待其完成。".to_string())
        } else {
            Ok(Self { flag: flag.clone() })
        }
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

pub fn data_root() -> PathBuf {
    config::home_dir().join("data")
}

fn data_folder(is_rt: bool) -> PathBuf {
    data_root().join(if is_rt { "RT" } else { "BT" })
}

fn backup_root() -> PathBuf {
    config::home_dir().join("data_bak")
}

pub fn backup_code(root_path: &str, is_rt: bool, mia_version: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(root_path.as_bytes());
    if is_rt {
        hasher.update(mia_version.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn backup_parent(root_path: &str, is_rt: bool, mia_version: &str) -> PathBuf {
    backup_root().join(backup_code(root_path, is_rt, mia_version))
}

fn sorted_entries(folder: &Path) -> Vec<(PathBuf, String, bool)> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut items: Vec<(PathBuf, String, bool)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let metadata = fs::metadata(&path).ok()?;
            Some((path, name, metadata.is_dir()))
        })
        .collect();
    // 文件夹优先，其次按名称排序（与 Electron 版一致）。
    items.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    items
}

fn read_tree(folder: &Path, level: u32, result: &mut Vec<FileNode>) {
    for (path, name, is_folder) in sorted_entries(folder) {
        let size = if is_folder {
            None
        } else {
            fs::metadata(&path).ok().map(|meta| meta.len())
        };
        result.push(FileNode {
            id: result.len() + 1,
            name,
            path: path.to_string_lossy().to_string(),
            is_folder,
            level,
            size,
            status: None,
            error: None,
        });
        if is_folder {
            read_tree(&path, level + 1, result);
        }
    }
}

fn collect_files(folder: &Path, result: &mut Vec<PathBuf>) {
    for (path, _name, is_folder) in sorted_entries(folder) {
        if is_folder {
            collect_files(&path, result);
        } else {
            result.push(path);
        }
    }
}

pub fn data_info(is_rt: bool) -> DataInfo {
    let root = data_root();
    let version = fs::read_to_string(root.join("version"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let mut files = Vec::new();
    if root.is_dir() {
        read_tree(&data_folder(is_rt), 0, &mut files);
    }
    DataInfo { version, files }
}

pub fn list_backups(root_path: &str, is_rt: bool, mia_version: &str) -> Vec<BackupEntry> {
    let parent = backup_parent(root_path, is_rt, mia_version);
    let Ok(entries) = fs::read_dir(&parent) else {
        return Vec::new();
    };
    let mut items: Vec<(std::time::SystemTime, BackupEntry)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = fs::metadata(&path).ok()?;
            if !metadata.is_dir() {
                return None;
            }
            let created = metadata
                .created()
                .or_else(|_| metadata.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            Some((
                created,
                BackupEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: path.to_string_lossy().to_string(),
                },
            ))
        })
        .collect();
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.into_iter().map(|(_, entry)| entry).collect()
}

fn copy_preserve(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败 {}: {e}", parent.display()))?;
    }
    fs::copy(source, target).map_err(|e| {
        format!(
            "复制失败 {} -> {}: {e}",
            source.display(),
            target.display()
        )
    })?;
    if let Ok(metadata) = fs::metadata(source) {
        let atime = filetime::FileTime::from_last_access_time(&metadata);
        let mtime = filetime::FileTime::from_last_modification_time(&metadata);
        let _ = filetime::set_file_times(target, atime, mtime);
    }
    Ok(())
}

fn relative_to(path: &Path, base: &Path) -> PathBuf {
    path.strip_prefix(base)
        .map(|value| value.to_path_buf())
        .unwrap_or_else(|_| {
            path.file_name()
                .map(PathBuf::from)
                .unwrap_or_default()
        })
}

fn percent(completed: u32, total: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    ((completed as f64 / total as f64) * 100.0).floor().min(100.0) as u32
}

fn emit_progress(app: &AppHandle, phase: &str, completed: u32, total: u32) {
    let _ = app.emit(
        "loc-progress",
        ProgressEvent {
            phase: phase.to_string(),
            percent: percent(completed, total),
            completed,
            total,
        },
    );
}

fn emit_file(app: &AppHandle, path: &Path, status: &str, error: &str) {
    let _ = app.emit(
        "loc-file",
        FileStatusEvent {
            path: path.to_string_lossy().to_string(),
            status: status.to_string(),
            error: error.to_string(),
        },
    );
}

fn validate_game(root_path: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(root_path);
    if root_path.trim().is_empty() || !root.is_dir() {
        return Err("游戏目录不存在，请重新选择。".to_string());
    }
    Ok(root)
}

/// 修改 RtLauncherSettings.xml，将 `SafeLaunchDisabled` 设为 true。
pub fn update_rtl_settings(rtl_path: &str) -> Result<(), String> {
    let path = Path::new(rtl_path).join("RtLauncherSettings.xml");
    if !path.is_file() {
        return Err(format!("未找到配置文件{}", path.display()));
    }
    let original = fs::read_to_string(&path).map_err(|e| format!("读取配置失败: {e}"))?;
    let pair = regex::Regex::new(r"(?s)<SafeLaunchDisabled\b[^>]*>.*?</SafeLaunchDisabled>")
        .map_err(|e| e.to_string())?;
    let empty = regex::Regex::new(r"(?s)<SafeLaunchDisabled\b[^>]*/>").map_err(|e| e.to_string())?;
    let replacement = "<SafeLaunchDisabled>true</SafeLaunchDisabled>";

    let updated = if pair.is_match(&original) {
        pair.replace_all(&original, replacement).to_string()
    } else if empty.is_match(&original) {
        empty.replace_all(&original, replacement).to_string()
    } else {
        let closing = "</RogueTechLauncherSettings>";
        let Some(position) = original.rfind(closing) else {
            return Err("无法解析 RtLauncherSettings.xml 根节点".to_string());
        };
        format!(
            "{}\t{}\n{}",
            &original[..position],
            replacement,
            &original[position..]
        )
    };

    fs::write(&path, updated).map_err(|e| format!("写入配置失败: {e}"))?;
    Ok(())
}

fn apply_loc_json_file(root: &Path, loc_file: &Path) -> Result<(), String> {
    if !loc_file.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(loc_file).map_err(|e| e.to_string())?;
    let text = text.trim_start_matches('\u{feff}');
    let entries: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let Some(entries) = entries.as_array() else {
        return Ok(());
    };

    for entry in entries {
        let Some(filename) = entry.get("FileName").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(loc) = entry.get("Loc").and_then(|value| value.as_object()) else {
            continue;
        };
        if loc.is_empty() {
            continue;
        }
        let relative = filename.replace('\\', std::path::MAIN_SEPARATOR_STR);
        let file_path = root.join(relative);
        if !file_path.is_file() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&file_path) else {
            continue;
        };
        let content = content.trim_start_matches('\u{feff}');
        let Ok(mut data) = serde_json::from_str::<serde_json::Value>(content) else {
            continue;
        };
        let Some(items) = data.as_array_mut() else {
            continue;
        };
        for (key, value) in loc {
            if let Some(item) = items
                .iter_mut()
                .find(|item| item.get("Name").and_then(|name| name.as_str()) == Some(key.as_str()))
            {
                if let Some(localization) = item
                    .get_mut("Localization")
                    .and_then(|value| value.as_object_mut())
                {
                    if !localization.contains_key("CULTURE_ZH_CN") {
                        localization.insert("CULTURE_ZH_CN".to_string(), value.clone());
                    }
                }
            }
        }
        let output = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
        fs::write(&file_path, output).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn apply_loc_json(root: &Path) -> Result<(), String> {
    apply_loc_json_file(root, &data_root().join("loc.json"))
}

pub fn apply_localization(app: &AppHandle, request: &ApplyRequest) -> Result<TaskSummary, String> {
    if request.game_version != "1.9.1" {
        return Err(format!(
            "不支持的游戏版本: {}，目前只支持1.9.1版本的汉化。",
            request.game_version
        ));
    }
    if request.is_rt && !crate::game::scan_game(&request.root_path, &request.rtl_path).valid_rtl_folder
    {
        return Err("请先选择合法的RogueLauncher的根目录。".to_string());
    }

    let root = validate_game(&request.root_path)?;
    let folder = data_folder(request.is_rt);
    let mut files = Vec::new();
    collect_files(&folder, &mut files);
    if files.is_empty() {
        return Err("没有可用的汉化文件，请先下载汉化内容。".to_string());
    }

    if request.is_rt {
        update_rtl_settings(&request.rtl_path)?;
    }

    let auto_save = backup_parent(&request.root_path, request.is_rt, &request.mia_version)
        .join("auto_save");
    let total = files.len() as u32;
    let mut completed = 0u32;
    let mut done = 0u32;
    let mut failed = 0u32;

    for original in files {
        let relative = relative_to(&original, &folder);
        let target = root.join(&relative);
        let backup = auto_save.join(&relative);
        emit_file(app, &original, "processing", "");

        // STEAM 版 RT / GOG 版 BT 使用另一套 Assembly-CSharp.dll。
        let mut source = original.clone();
        if original.file_name().and_then(|name| name.to_str()) == Some("Assembly-CSharp.dll") {
            let text = original.to_string_lossy().to_string();
            if request.platform == "STEAM" && request.is_rt {
                if let Some(index) = text.rfind("\\RT\\") {
                    source = PathBuf::from(format!(
                        "{}\\BT\\{}",
                        &text[..index],
                        &text[index + 4..]
                    ));
                }
            } else if request.platform == "GOG" && request.is_bt {
                if let Some(index) = text.rfind("\\BT\\") {
                    source = PathBuf::from(format!(
                        "{}\\RT\\{}",
                        &text[..index],
                        &text[index + 4..]
                    ));
                }
            }
        }

        let result = (|| -> Result<(), String> {
            if target.is_file() {
                if !backup.is_file() {
                    copy_preserve(&target, &backup)?;
                }
                copy_preserve(&source, &target)
            } else {
                copy_preserve(&source, &target)
            }
        })();

        completed += 1;
        match result {
            Ok(()) => {
                done += 1;
                emit_file(app, &original, "done", "");
            }
            Err(error) => {
                failed += 1;
                emit_file(app, &original, "error", &error);
            }
        }
        emit_progress(app, "apply", completed, total);
    }

    // 与 Electron 版一致：RT 汉化后补齐 Mod 本地化文本，失败不影响主流程。
    if request.is_rt {
        if let Err(error) = apply_loc_json(&root) {
            eprintln!("写入 RogueTech 本地化 JSON 失败: {error}");
        }
    }

    Ok(TaskSummary {
        done,
        failed,
        total,
        time: None,
    })
}

pub fn backup_localization(app: &AppHandle, request: &BackupRequest) -> Result<TaskSummary, String> {
    let root = validate_game(&request.root_path)?;
    let folder = data_folder(request.is_rt);
    let mut files = Vec::new();
    collect_files(&folder, &mut files);
    if files.is_empty() {
        return Err("没有可用的汉化文件，请先下载汉化内容。".to_string());
    }

    let time = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    let backup_dir = backup_parent(&request.root_path, request.is_rt, &request.mia_version).join(&time);
    if backup_dir.exists() {
        return Err(format!("备份失败，已存在的目录: {}", backup_dir.display()));
    }

    let total = files.len() as u32;
    let mut completed = 0u32;
    let mut done = 0u32;
    let mut failed = 0u32;
    for file in files {
        let relative = relative_to(&file, &folder);
        let target = root.join(&relative);
        if target.is_file() {
            match copy_preserve(&target, &backup_dir.join(&relative)) {
                Ok(()) => done += 1,
                Err(error) => {
                    failed += 1;
                    emit_file(app, &file, "error", &error);
                }
            }
        } else {
            // Electron 版对不存在的目标文件也计为完成（无需备份）。
            done += 1;
        }
        completed += 1;
        emit_progress(app, "backup", completed, total);
    }

    Ok(TaskSummary {
        done,
        failed,
        total,
        time: Some(time),
    })
}

pub fn restore_localization(
    app: &AppHandle,
    request: &RestoreRequest,
) -> Result<TaskSummary, String> {
    let root = validate_game(&request.root_path)?;
    if !is_safe_component(&request.time) {
        return Err("非法的备份名称。".to_string());
    }
    let backup_dir =
        backup_parent(&request.root_path, request.is_rt, &request.mia_version).join(&request.time);
    if !backup_dir.is_dir() {
        return Err(format!("恢复失败，备份目录不存在: {}", request.time));
    }

    let files: Vec<PathBuf> = WalkDir::new(&backup_dir)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect();
    let total = files.len() as u32;
    let mut completed = 0u32;
    let mut done = 0u32;
    let mut failed = 0u32;
    for file in files {
        let relative = relative_to(&file, &backup_dir);
        match copy_preserve(&file, &root.join(&relative)) {
            Ok(()) => done += 1,
            Err(error) => {
                failed += 1;
                emit_file(app, &file, "error", &error);
            }
        }
        completed += 1;
        emit_progress(app, "restore", completed, total);
    }

    Ok(TaskSummary {
        done,
        failed,
        total,
        time: Some(request.time.clone()),
    })
}

pub fn delete_backup(request: &DeleteBackupRequest) -> Result<(), String> {
    if !is_safe_component(&request.name) {
        return Err("非法的备份名称。".to_string());
    }
    let backup_dir = backup_parent(&request.root_path, request.is_rt, &request.mia_version)
        .join(&request.name);
    if backup_dir.is_dir() {
        fs::remove_dir_all(&backup_dir).map_err(|e| format!("删除备份失败: {e}"))?;
    }
    Ok(())
}

fn is_safe_component(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name
            .chars()
            .any(|ch| matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

fn http_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

fn api_url(path: &str) -> String {
    format!("http://{}:{}{}", crate::system::API_HOST, crate::system::API_PORT, path)
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() || error.is_connect() {
        "连接服务器失败，若确认网络正常，则可能是远程服务器当前已离线，请联系软件作者以解决此问题。"
            .to_string()
    } else {
        error.to_string()
    }
}

/// 获取服务器最新汉化内容版本。
pub async fn latest_version() -> Result<String, String> {
    let client = http_client(Duration::from_secs(10))?;
    let response = client
        .get(api_url("/api/latest"))
        .send()
        .await
        .map_err(network_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP错误: {}", status.as_u16()));
    }
    let value: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    if !value.get("success").and_then(|item| item.as_bool()).unwrap_or(false) {
        return Err(value
            .get("message")
            .and_then(|item| item.as_str())
            .unwrap_or("接口错误")
            .to_string());
    }
    Ok(value
        .get("content")
        .and_then(|item| item.as_str())
        .unwrap_or_default()
        .trim()
        .to_string())
}

async fn download_archive(app: &AppHandle, url: &str, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let client = http_client(Duration::from_secs(60 * 30))?;
    let response = client.get(url).send().await.map_err(network_error)?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP错误: {}", status.as_u16()));
    }
    let total = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(target)
        .await
        .map_err(|e| format!("创建下载文件失败: {e}"))?;
    let mut loaded: u64 = 0;
    let mut last_percent = 101u32;

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                drop(file);
                let _ = tokio::fs::remove_file(target).await;
                return Err(network_error(error));
            }
        };
        if let Err(error) = file.write_all(&chunk).await {
            drop(file);
            let _ = tokio::fs::remove_file(target).await;
            return Err(format!("写入下载文件失败: {error}"));
        }
        loaded += chunk.len() as u64;
        if total > 0 {
            let current = ((loaded as f64 / total as f64) * 100.0).round().min(100.0) as u32;
            if current != last_percent {
                last_percent = current;
                let _ = app.emit(
                    "loc-progress",
                    ProgressEvent {
                        phase: "download".to_string(),
                        percent: current,
                        completed: loaded.min(u32::MAX as u64) as u32,
                        total: total.min(u32::MAX as u64) as u32,
                    },
                );
            }
        }
    }
    if let Err(error) = file.flush().await {
        drop(file);
        let _ = tokio::fs::remove_file(target).await;
        return Err(format!("写入下载文件失败: {error}"));
    }
    Ok(())
}

fn remove_path(path: &Path) -> Result<(), String> {
    if path.is_dir() {
        fs::remove_dir_all(path).map_err(|e| format!("删除目录失败 {}: {e}", path.display()))
    } else if path.exists() {
        fs::remove_file(path).map_err(|e| format!("删除文件失败 {}: {e}", path.display()))
    } else {
        Ok(())
    }
}

fn copy_dir_all(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|e| format!("创建目录失败: {e}"))?;
    for entry in WalkDir::new(source).into_iter().filter_map(|item| item.ok()) {
        let path = entry.path();
        let relative = relative_to(path, source);
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|e| format!("创建目录失败: {e}"))?;
        } else {
            copy_preserve(path, &destination)?;
        }
    }
    Ok(())
}

fn extract_and_install(app: &AppHandle, archive: &Path, expected: &str) -> Result<String, String> {
    let home = config::home_dir();
    let tmp_dir = home.join("data_tmp");
    let data_dir = home.join("data");
    remove_path(&tmp_dir).map_err(|e| format!("清理临时目录失败: {e}"))?;
    emit_progress(app, "extract", 0, 1);
    sevenzip::extract(archive, &tmp_dir)?;

    let version = fs::read_to_string(tmp_dir.join("version"))
        .unwrap_or_default()
        .trim()
        .to_string();
    if !expected.trim().is_empty() && !version.is_empty() && version != expected.trim() {
        eprintln!("警告: 下载包版本 {version} 与期望版本 {expected} 不一致");
    }

    remove_path(&data_dir).map_err(|e| format!("清理旧汉化数据失败: {e}"))?;
    if fs::rename(&tmp_dir, &data_dir).is_err() {
        copy_dir_all(&tmp_dir, &data_dir)?;
        let _ = fs::remove_dir_all(&tmp_dir);
    }
    emit_progress(app, "extract", 1, 1);
    Ok(version)
}

/// 下载并安装汉化内容；若已有同版本临时包则直接解压。
pub async fn install_localization(
    app: AppHandle,
    busy: Arc<AtomicBool>,
    machine_id: String,
    app_version: String,
    expected: String,
) -> Result<String, String> {
    let _guard = BusyGuard::acquire(&busy)?;
    let home = config::home_dir();
    fs::create_dir_all(&home).map_err(|e| format!("创建数据目录失败: {e}"))?;
    let archive = home.join("data_tmp.7z");
    let mut need_download = true;

    if archive.is_file() {
        let check_path = archive.clone();
        let existing = tokio::task::spawn_blocking(move || sevenzip::archive_version(&check_path))
            .await
            .map_err(|e| format!("读取临时包失败: {e}"))?;
        if existing.as_deref() == Some(expected.trim()) && !expected.trim().is_empty() {
            need_download = false;
        } else {
            let _ = fs::remove_file(&archive);
        }
    }

    if need_download {
        let url = format!(
            "http://{}:{}/api/download?machineId={}&appVersion={}",
            crate::system::API_HOST,
            crate::system::API_PORT,
            urlencoding::encode(&machine_id),
            urlencoding::encode(&app_version)
        );
        download_archive(&app, &url, &archive).await?;
    }

    let app_for_extract = app.clone();
    let archive_for_extract = archive.clone();
    let expected_for_extract = expected.clone();
    tokio::task::spawn_blocking(move || {
        extract_and_install(&app_for_extract, &archive_for_extract, &expected_for_extract)
    })
    .await
    .map_err(|e| format!("解压任务失败: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_code_matches_md5() {
        assert_eq!(backup_code("abc", false, ""), "900150983cd24fb0d6963f7d28e17f72");
        assert_ne!(
            backup_code(r"E:\Games\BATTLETECH", true, "MIA 1.1.7"),
            backup_code(r"E:\Games\BATTLETECH", false, "")
        );
    }

    #[test]
    fn rtl_settings_adds_and_replaces_safe_launch() {
        let dir = std::env::temp_dir().join(format!("bth-rtl-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("RtLauncherSettings.xml");
        fs::write(
            &file,
            "<?xml version=\"1.0\" ?>\n<RogueTechLauncherSettings>\n\t<Banner>Banner-C</Banner>\n</RogueTechLauncherSettings>\n",
        )
        .unwrap();
        update_rtl_settings(dir.to_str().unwrap()).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("<SafeLaunchDisabled>true</SafeLaunchDisabled>"));

        fs::write(
            &file,
            "<RogueTechLauncherSettings><SafeLaunchDisabled>false</SafeLaunchDisabled></RogueTechLauncherSettings>",
        )
        .unwrap();
        update_rtl_settings(dir.to_str().unwrap()).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.contains("<SafeLaunchDisabled>true</SafeLaunchDisabled>"));
        assert!(!text.contains("false"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn loc_json_only_fills_missing_chinese_text() {
        let dir = std::env::temp_dir().join(format!("bth-loc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Mods/Core/Test")).unwrap();
        let game_file = dir.join("Mods/Core/Test/Localization.json");
        fs::write(
            &game_file,
            r#"[{"Name":"hello","Localization":{"CULTURE_EN":"Hello","CULTURE_ZH_CN":"已有"}},{"Name":"world","Localization":{"CULTURE_EN":"World"}}]"#,
        )
        .unwrap();
        let loc_file = dir.join("loc.json");
        fs::write(
            &loc_file,
            r#"[{"FileName":"Mods\\Core\\Test\\Localization.json","Loc":{"hello":"你好","world":"世界"}}]"#,
        )
        .unwrap();
        apply_loc_json_file(&dir, &loc_file).unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&game_file).unwrap()).unwrap();
        assert_eq!(value[0]["Localization"]["CULTURE_ZH_CN"], "已有");
        assert_eq!(value[1]["Localization"]["CULTURE_ZH_CN"], "世界");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_path_traversal_backup_name() {
        assert!(!is_safe_component("..\\evil"));
        assert!(!is_safe_component("a/b"));
        assert!(is_safe_component("20260101120000"));
    }
}
