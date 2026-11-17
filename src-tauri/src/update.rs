//! 自动更新：沿用 Electron 版的服务端协议。
//!
//! 检查 `http://battletech.natapp1.cc/update/win32/latest.yml`，
//! 版本说明读取 `/api/version/{version}_default`，安装包从同一 feed 下载后启动。

use crate::config;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingUpdate {
    pub version: String,
    pub path: String,
    pub change: String,
}

fn platform_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "win32"
    }
    #[cfg(target_os = "macos")]
    {
        "darwin"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "linux"
    }
}

fn feed_url() -> String {
    format!(
        "http://{}:{}/update/{}",
        crate::system::API_HOST,
        crate::system::API_PORT,
        platform_name()
    )
}

fn send(app: &AppHandle, event: &str, data: Option<Value>) {
    let mut payload = json!({ "event": event });
    if let Some(data) = data {
        payload["data"] = data;
    }
    let _ = app.emit("update-msg", payload);
}

fn http_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

fn yaml_value(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(&prefix) else {
            continue;
        };
        let value = rest.trim().trim_matches('"').trim_matches('\'').trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .trim_start_matches('v')
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        })
        .collect()
}

pub fn is_newer(current: &str, latest: &str) -> bool {
    let current = version_parts(current);
    let latest = version_parts(latest);
    let length = current.len().max(latest.len());
    for index in 0..length {
        let left = *current.get(index).unwrap_or(&0);
        let right = *latest.get(index).unwrap_or(&0);
        if right > left {
            return true;
        }
        if right < left {
            return false;
        }
    }
    false
}

/// 检查更新，事件格式与 Electron 版渲染进程一致。
pub async fn check_update(
    app: AppHandle,
    pending_update: Arc<Mutex<Option<PendingUpdate>>>,
) -> Result<(), String> {
    send(&app, "checking", None);
    let client = http_client(Duration::from_secs(15))?;
    let url = format!("{}/latest.yml", feed_url());
    let response = match client.get(&url).send().await {
        Ok(response) => response,
        Err(error) => {
            let message = if error.is_timeout() || error.is_connect() {
                "连接更新服务器失败，请检查网络。".to_string()
            } else {
                error.to_string()
            };
            send(&app, "error", Some(json!({ "msg": message })));
            return Err(message);
        }
    };

    let status = response.status();
    if status.as_u16() == 404 {
        // Electron 版的 "Cannot find channel latest.yml" 也按“已是最新”处理。
        send(&app, "notAva", None);
        return Ok(());
    }
    if !status.is_success() {
        let message = format!("更新服务器返回 HTTP {}", status.as_u16());
        send(&app, "error", Some(json!({ "msg": message })));
        return Err(message);
    }

    let text = response.text().await.map_err(|e| e.to_string())?;
    let Some(latest) = yaml_value(&text, "version") else {
        send(&app, "notAva", None);
        return Ok(());
    };
    if !is_newer(crate::system::APP_VERSION, &latest) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        send(&app, "notAva", None);
        return Ok(());
    }

    let path = yaml_value(&text, "path")
        .or_else(|| yaml_value(&text, "url"))
        .unwrap_or_default();
    let change_url = format!(
        "http://{}:{}/api/version/{}_default",
        crate::system::API_HOST,
        crate::system::API_PORT,
        urlencoding::encode(&latest)
    );
    let change = match client.get(change_url).send().await {
        Ok(response) if response.status().is_success() => {
            match response.json::<Value>().await {
                Ok(value) => value
                    .get("notes")
                    .and_then(|item| item.as_str())
                    .unwrap_or_default()
                    .to_string(),
                Err(_) => String::new(),
            }
        }
        _ => String::new(),
    };

    if let Ok(mut guard) = pending_update.lock() {
        *guard = Some(PendingUpdate {
            version: latest.clone(),
            path,
            change: change.clone(),
        });
    }
    send(
        &app,
        "ava",
        Some(json!({ "version": latest, "change": change })),
    );
    Ok(())
}

fn update_dir() -> PathBuf {
    config::home_dir().join("update")
}

fn installer_file_name(path: &str) -> String {
    let name = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("BATTLETECH-hanhua-setup.exe");
    if name.to_lowercase().ends_with(".exe") {
        name.to_string()
    } else {
        format!("{name}.exe")
    }
}

async fn download_installer(
    app: &AppHandle,
    url: &str,
    target: &Path,
) -> Result<(), String> {
    let client = http_client(Duration::from_secs(60 * 30))?;
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP错误: {}", status.as_u16()));
    }
    let total = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(target)
        .await
        .map_err(|e| format!("创建更新文件失败: {e}"))?;
    let mut loaded = 0u64;
    let mut last = 101u32;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写入更新文件失败: {e}"))?;
        loaded += chunk.len() as u64;
        if total > 0 {
            let current = ((loaded as f64 / total as f64) * 100.0).round().min(100.0) as u32;
            if current != last {
                last = current;
                send(app, "progress", Some(json!({ "percent": current })));
            }
        }
    }
    file.flush()
        .await
        .map_err(|e| format!("写入更新文件失败: {e}"))?;
    Ok(())
}

/// 下载暂存的最新版本并启动安装程序。
pub async fn install_update(
    app: AppHandle,
    pending_update: Arc<Mutex<Option<PendingUpdate>>>,
) -> Result<(), String> {
    let pending = pending_update
        .lock()
        .ok()
        .and_then(|mut guard| guard.take());
    let Some(pending) = pending else {
        return Err("没有待安装的更新，请先检查更新。".to_string());
    };
    if pending.path.trim().is_empty() {
        return Err("更新包地址为空，无法下载。".to_string());
    }

    let url = if pending.path.starts_with("http://") || pending.path.starts_with("https://") {
        pending.path.clone()
    } else {
        format!("{}/{}", feed_url(), urlencoding::encode(&pending.path))
    };
    let directory = update_dir();
    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|e| format!("创建更新目录失败: {e}"))?;
    let target = directory.join(installer_file_name(&pending.path));
    send(&app, "startUpdate", None);

    if let Err(error) = download_installer(&app, &url, &target).await {
        send(&app, "error", Some(json!({ "msg": error })));
        return Err(error);
    }
    send(&app, "downloaded", None);

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new(&target)
            .spawn()
            .map_err(|e| format!("启动安装程序失败: {e}"))?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        app.exit(0);
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = target;
        app.exit(0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("1.0.1", "1.0.2"));
        assert!(is_newer("1.0.1", "1.1.0"));
        assert!(!is_newer("1.0.2", "1.0.2"));
        assert!(!is_newer("1.1.0", "1.0.9"));
        assert!(is_newer("1.9", "1.10.0"));
    }

    #[test]
    fn parses_latest_yml_fields() {
        let text = "version: 1.0.2\nfiles:\n  - url: BATTLETECH-setup.exe\npath: BATTLETECH-setup.exe\n";
        assert_eq!(yaml_value(text, "version").as_deref(), Some("1.0.2"));
        assert_eq!(
            yaml_value(text, "path").as_deref(),
            Some("BATTLETECH-setup.exe")
        );
    }

    #[test]
    fn sanitizes_installer_name() {
        assert_eq!(installer_file_name("a/b/setup.exe"), "setup.exe");
        assert_eq!(installer_file_name("setup"), "setup.exe");
    }
}
