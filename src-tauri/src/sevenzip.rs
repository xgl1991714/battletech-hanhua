//! 纯 Rust 的 7z 读取/解压（替代 Electron 版外置的 7za.exe）。

use std::path::Path;

/// 读取 7z 包根目录下 `version` 文件内容；不存在或解析失败返回 None。
pub fn archive_version(archive: &Path) -> Option<String> {
    if !archive.is_file() {
        return None;
    }
    let mut reader =
        sevenz_rust2::ArchiveReader::open(archive, sevenz_rust2::Password::empty()).ok()?;
    let data = reader.read_file("version").ok()?;
    let text = String::from_utf8(data).ok()?;
    Some(text.trim().to_string())
}

/// 解压到目标目录（目标目录会自动创建）。
pub fn extract(archive: &Path, output_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("创建解压目录失败: {e}"))?;
    sevenz_rust2::decompress_file(archive, output_dir)
        .map_err(|e| format!("解压失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_archive_has_no_version() {
        assert_eq!(archive_version(Path::new("not-exists.7z")), None);
    }
}
