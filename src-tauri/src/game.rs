//! 游戏版本、RT 状态与 RogueLauncher 版本识别。

use serde::Serialize;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameStatus {
    pub root_path: String,
    pub rtl_path: String,
    pub is_bt: bool,
    pub is_rt: bool,
    pub alias_name: String,
    pub mia_version: String,
    pub game_version: String,
    pub platform: String,
    pub rtl_version: String,
    pub valid_rtl_folder: bool,
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn json_string(value: &serde_json::Value, key: &str) -> String {
    match value.get(key) {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn read_game_version(root: &Path) -> (String, String) {
    let path = root
        .join("BattleTech_Data")
        .join("StreamingAssets")
        .join("version.json");
    if !path.is_file() {
        return (String::new(), String::new());
    }
    match read_json(&path) {
        Some(value) => (
            json_string(&value, "ProductVersion"),
            json_string(&value, "Platform"),
        ),
        None => (String::new(), String::new()),
    }
}

fn read_mia_version(root: &Path) -> String {
    let path = root
        .join("Mods")
        .join("Core")
        .join("RogueTechCore")
        .join("mod.json");
    if !path.is_file() {
        return String::new();
    }
    read_json(&path)
        .map(|value| json_string(&value, "Version"))
        .unwrap_or_default()
}

fn valid_rtl_folder(rtl_path: &str) -> bool {
    if rtl_path.trim().is_empty() {
        return false;
    }
    let folder = Path::new(rtl_path);
    folder.is_dir() && folder.join("RtLauncherSettings.xml").is_file()
}

pub fn scan_game(root_path: &str, rtl_path: &str) -> GameStatus {
    let root = Path::new(root_path);
    let is_bt = !root_path.trim().is_empty() && root.join("BattleTech.exe").is_file();
    let is_rt = is_bt
        && root
            .join("Mods")
            .join("roguelauncherData.ibin")
            .is_file();
    let (game_version, platform) = if is_bt {
        read_game_version(root)
    } else {
        (String::new(), String::new())
    };
    let mia_version = if is_rt {
        read_mia_version(root)
    } else {
        String::new()
    };
    let rtl_version = if !rtl_path.trim().is_empty() {
        let exe = Path::new(rtl_path).join("RogueLauncher.exe");
        if exe.is_file() {
            parse_rogue_launcher_version(&exe)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let alias_name = if is_rt {
        "RT".to_string()
    } else if is_bt {
        "BT".to_string()
    } else {
        String::new()
    };

    GameStatus {
        root_path: root_path.to_string(),
        rtl_path: rtl_path.to_string(),
        is_bt,
        is_rt,
        alias_name,
        mia_version,
        game_version,
        platform,
        rtl_version,
        valid_rtl_folder: valid_rtl_folder(rtl_path),
    }
}

#[cfg(target_os = "windows")]
fn get_steam_path() -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Valve\\Steam")
        .ok()?;
    let value: String = key.get_value("SteamPath").ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value.replace('/', "\\")))
    }
}

#[cfg(not(target_os = "windows"))]
fn get_steam_path() -> Option<PathBuf> {
    None
}

#[cfg(target_os = "windows")]
fn get_steam_libraries(steam_path: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![steam_path.to_path_buf()];
    let library_file = steam_path.join("steamapps").join("libraryfolders.vdf");
    let Ok(content) = fs::read_to_string(&library_file) else {
        return libraries;
    };
    let Ok(regex) = regex::Regex::new(r#""path"\s+"([^"]+)""#) else {
        return libraries;
    };
    for captures in regex.captures_iter(&content) {
        let Some(value) = captures.get(1) else {
            continue;
        };
        let library = PathBuf::from(value.as_str().replace("\\\\", "\\"));
        if !libraries.iter().any(|item| item == &library) {
            libraries.push(library);
        }
    }
    libraries
}

#[cfg(not(target_os = "windows"))]
fn get_steam_libraries(_steam_path: &Path) -> Vec<PathBuf> {
    Vec::new()
}

/// 通过 Steam 注册表与 libraryfolders.vdf 自动定位 BATTLETECH。
pub fn find_battle_tech() -> Option<String> {
    let steam_path = get_steam_path()?;
    for library in get_steam_libraries(&steam_path) {
        let game_path = library
            .join("steamapps")
            .join("common")
            .join("BATTLETECH");
        if game_path.is_dir() {
            return Some(game_path.to_string_lossy().to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn find_rogue_launcher_windows() -> Option<String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(
            "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FeatureUsage\\AppLaunch",
        )
        .ok()?;

    for value in key.enum_values().flatten() {
        let (name, _) = value;
        if !name.to_lowercase().contains("roguelauncher.exe") {
            continue;
        }
        let candidate = if name.to_lowercase().ends_with(".exe") {
            name
        } else {
            key.get_value::<String, _>(name.as_str()).unwrap_or_default()
        };
        let candidate = candidate.trim().trim_matches('"');
        if candidate.is_empty() {
            continue;
        }
        let path = Path::new(candidate);
        if let Some(parent) = path.parent() {
            return Some(parent.to_string_lossy().to_string());
        }
    }
    None
}

/// 通过 Windows AppLaunch 注册表项定位 RogueLauncher。
pub fn find_rogue_launcher() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        find_rogue_launcher_windows()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn read_exact_at(file: &mut fs::File, position: u64, length: usize) -> std::io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(position))?;
    let mut buffer = vec![0u8; length];
    file.read_exact(&mut buffer)?;
    Ok(buffer)
}

/// 解析 PyInstaller one-file 打包的 `RogueLauncher.exe` 内的版本号。
///
/// 与 Electron 版实现一致：读取文件尾部 88 字节 Cookie，定位 `srtlauncher`
/// 条目，zlib 解压后在 Python 字节码字符串中匹配版本号。
pub fn parse_rogue_launcher_version(file_path: &Path) -> String {
    let Ok(mut file) = fs::File::open(file_path) else {
        return String::new();
    };
    let Ok(file_size) = file.metadata().map(|meta| meta.len()) else {
        return String::new();
    };

    const COOKIE_SIZE: u64 = 88;
    if file_size < COOKIE_SIZE {
        return String::new();
    }

    let Ok(cookie) = read_exact_at(&mut file, file_size - COOKIE_SIZE, COOKIE_SIZE as usize) else {
        return String::new();
    };
    const MAGIC: [u8; 8] = [0x4d, 0x45, 0x49, 0x0c, 0x0b, 0x0a, 0x0b, 0x0e];
    if cookie[..8] != MAGIC[..] {
        return String::new();
    }

    let package_length = u32::from_be_bytes([cookie[8], cookie[9], cookie[10], cookie[11]]) as u64;
    let toc_position = u32::from_be_bytes([cookie[12], cookie[13], cookie[14], cookie[15]]) as u64;
    let toc_length = u32::from_be_bytes([cookie[16], cookie[17], cookie[18], cookie[19]]) as u64;

    if package_length > file_size {
        return String::new();
    }
    let package_start = file_size - package_length;
    let toc_absolute = package_start.saturating_add(toc_position);
    if toc_absolute.saturating_add(toc_length) > file_size {
        return String::new();
    }
    let Ok(toc) = read_exact_at(&mut file, toc_absolute, toc_length as usize) else {
        return String::new();
    };

    let mut offset = 0usize;
    let mut target: Option<(u32, u32)> = None;
    while offset + 4 <= toc.len() {
        let entry_size =
            u32::from_be_bytes([toc[offset], toc[offset + 1], toc[offset + 2], toc[offset + 3]])
                as usize;
        if entry_size < 18 || offset + entry_size > toc.len() {
            return String::new();
        }
        let position = u32::from_be_bytes([
            toc[offset + 4],
            toc[offset + 5],
            toc[offset + 6],
            toc[offset + 7],
        ]);
        let compressed_size = u32::from_be_bytes([
            toc[offset + 8],
            toc[offset + 9],
            toc[offset + 10],
            toc[offset + 11],
        ]);
        let name_start = offset + 17;
        let name_end = offset + entry_size;
        let name_bytes = &toc[name_start..name_end];
        let name_end = name_bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(name_bytes.len());
        let name = String::from_utf8_lossy(&name_bytes[..name_end]);
        if &*name == "srtlauncher" {
            target = Some((position, compressed_size));
            break;
        }
        offset += entry_size;
    }

    let Some((position, compressed_size)) = target else {
        return String::new();
    };
    let Ok(compressed) = read_exact_at(
        &mut file,
        package_start + position as u64,
        compressed_size as usize,
    ) else {
        return String::new();
    };

    let mut decoder = flate2::read::ZlibDecoder::new(&compressed[..]);
    let mut data = Vec::new();
    if decoder.read_to_end(&mut data).is_err() {
        return String::new();
    }
    // Python 字节码字符串按 latin1 处理，避免非 UTF-8 字符串导致匹配失败。
    let text: String = data.iter().map(|byte| *byte as char).collect();

    static VERSION_REGEX: OnceLock<regex::Regex> = OnceLock::new();
    let regex = VERSION_REGEX
        .get_or_init(|| regex::Regex::new(r"\d+\.\d+\.\d+(?:-\d{8})?").expect("版本号正则"));
    let matches: Vec<&str> = regex.find_iter(&text).map(|item| item.as_str()).collect();
    if matches.is_empty() {
        return String::new();
    }
    matches
        .iter()
        .find(|value| {
            value
                .split_once('-')
                .map(|(_, date)| date.len() == 8 && date.chars().all(|ch| ch.is_ascii_digit()))
                .unwrap_or(false)
        })
        .copied()
        .unwrap_or(matches[0])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_game_folder_is_not_bt() {
        let status = scan_game(r"Z:\definitely-not-a-game", "");
        assert!(!status.is_bt);
        assert!(!status.is_rt);
        assert_eq!(status.alias_name, "");
    }

    #[test]
    fn parses_installed_launcher_when_available() {
        let Ok(path) = std::env::var("ROGUELAUNCHER_EXE") else {
            return;
        };
        let version = parse_rogue_launcher_version(Path::new(&path));
        assert!(!version.is_empty(), "应当能解析 RogueLauncher 版本");
        assert!(version.starts_with(|ch: char| ch.is_ascii_digit()));
    }
}
