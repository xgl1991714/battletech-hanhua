//! 扫描 `Mods` 目录，提供机甲/载具/武器/装备数据查询。

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::{DirEntry, WalkDir};

const CACHE_VERSION: u32 = 16;
const CACHE_FILE_NAME: &str = "data_query_cache.json";

#[derive(Clone, Serialize, Deserialize)]
pub struct DataEntry {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub subtitle: String,
    #[serde(rename = "plainText")]
    pub plain_text: String,
    #[serde(rename = "searchText")]
    pub search_text: String,
    pub path: String,
}

pub struct DataCache {
    pub root: String,
    pub entries: Vec<DataEntry>,
    chassis_paths: HashMap<String, PathBuf>,
    vehicle_chassis_paths: HashMap<String, PathBuf>,
    component_paths: HashMap<String, PathBuf>,
    icon_paths: HashMap<String, PathBuf>,
    color_tags: HashMap<String, String>,
    category_color_tags: HashMap<String, String>,
    category_background_tags: std::collections::HashSet<String>,
    category_sorters: HashMap<String, i64>,
    affinities: Vec<Value>,
    mech_defaults: Option<Value>,
}

#[derive(Serialize, Deserialize)]
struct PersistentCache {
    version: u32,
    root: String,
    mods_stamp: Option<u64>,
    entries: Vec<DataEntry>,
    chassis_paths: HashMap<String, PathBuf>,
    vehicle_chassis_paths: HashMap<String, PathBuf>,
    component_paths: HashMap<String, PathBuf>,
    icon_paths: HashMap<String, PathBuf>,
    #[serde(default)]
    color_tags: HashMap<String, String>,
    #[serde(default)]
    category_color_tags: HashMap<String, String>,
    #[serde(default)]
    category_background_tags: std::collections::HashSet<String>,
    #[serde(default)]
    category_sorters: HashMap<String, i64>,
    affinities: Vec<Value>,
    mech_defaults: Option<Value>,
}

fn pointer_str<'a>(value: &'a Value, path: &[&str]) -> &'a str {
    let mut current = value;
    for key in path {
        current = match current.get(*key) {
            Some(next) => next,
            None => return "",
        };
    }
    current.as_str().unwrap_or("")
}

fn as_f64(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn as_i64(value: &Value, key: &str) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(0)
}

fn plain_text(input: &str) -> String {
    let mut text = String::with_capacity(input.len());
    let mut in_tag = false;
    for character in input.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(character),
            _ => {}
        }
    }
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn display_name(value: &Value, fallback: &str) -> String {
    let name = pointer_str(value, &["Description", "UIName"]);
    if name.is_empty() {
        let name = pointer_str(value, &["Description", "Name"]);
        if name.is_empty() {
            fallback.to_string()
        } else {
            name.to_string()
        }
    } else {
        name.to_string()
    }
}

fn def_id(value: &Value) -> String {
    let id = pointer_str(value, &["Description", "Id"]);
    if id.is_empty() {
        String::new()
    } else {
        id.to_string()
    }
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn read_json(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn skip_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    matches!(
        name.as_str(),
        ".modtek" | "saves" | "save" | "cache" | "localization" | "localizations"
    )
}

fn insert_path(map: &mut HashMap<String, PathBuf>, key: &str, path: &Path) {
    if !key.is_empty() {
        map.insert(key.to_string(), path.to_path_buf());
    }
    let stem = file_stem(path);
    if !stem.is_empty() {
        map.insert(stem, path.to_path_buf());
    }
}

fn component_prefix(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("gear_")
        || lower.starts_with("default_")
        || lower.starts_with("quirk_")
        || lower.starts_with("unique_")
        || lower.starts_with("special_")
        || lower.starts_with("linked_")
        || lower.starts_with("bolton_")
        || lower.starts_with("ammo_")
        || lower.starts_with("emod_")
        || lower.starts_with("upgrade_")
        || lower.starts_with("equipment_")
}

fn is_component(value: &Value) -> bool {
    let ui_name = pointer_str(value, &["Description", "UIName"]);
    let component_type = pointer_str(value, &["ComponentType"]);
    !ui_name.is_empty() && !component_type.is_empty()
}

fn cache_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join(CACHE_FILE_NAME))
}

/// 缓存失效只依据 Mods/roguelauncherData.ibin 的修改时间。
fn mods_stamp(root: &str) -> Option<u64> {
    let path = Path::new(root)
        .join("Mods")
        .join("roguelauncherData.ibin");
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(duration.as_nanos() as u64)
}

fn read_cache(root: &str, stamp: Option<u64>) -> Option<DataCache> {
    let path = cache_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    let cached: PersistentCache = serde_json::from_str(&text).ok()?;
    if cached.version != CACHE_VERSION
        || !cached.root.eq_ignore_ascii_case(root)
        || cached.mods_stamp != stamp
    {
        return None;
    }

    Some(DataCache {
        root: cached.root,
        entries: cached.entries,
        chassis_paths: cached.chassis_paths,
        vehicle_chassis_paths: cached.vehicle_chassis_paths,
        component_paths: cached.component_paths,
        icon_paths: cached.icon_paths,
        color_tags: cached.color_tags,
        category_color_tags: cached.category_color_tags,
        category_background_tags: cached.category_background_tags,
        category_sorters: cached.category_sorters,
        affinities: cached.affinities,
        mech_defaults: cached.mech_defaults,
    })
}

fn write_cache(cache: &DataCache, stamp: Option<u64>) {
    let Some(path) = cache_path() else {
        return;
    };
    let cached = PersistentCache {
        version: CACHE_VERSION,
        root: cache.root.clone(),
        mods_stamp: stamp,
        entries: cache.entries.clone(),
        chassis_paths: cache.chassis_paths.clone(),
        vehicle_chassis_paths: cache.vehicle_chassis_paths.clone(),
        component_paths: cache.component_paths.clone(),
        icon_paths: cache.icon_paths.clone(),
        color_tags: cache.color_tags.clone(),
        category_color_tags: cache.category_color_tags.clone(),
        category_background_tags: cache.category_background_tags.clone(),
        category_sorters: cache.category_sorters.clone(),
        affinities: cache.affinities.clone(),
        mech_defaults: cache.mech_defaults.clone(),
    };
    if let Ok(text) = serde_json::to_string(&cached) {
        let _ = std::fs::write(path, text);
    }
}

fn builtin_texture_name(icon: &str) -> Option<String> {
    icon.strip_prefix("uixSvgIcon_equipment_")
        .map(|suffix| format!("uixTxrIcon_gear_{suffix}"))
}

fn icon_cache_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.join("icon-cache");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn safe_icon_file_name(icon: &str) -> String {
    let name: String = icon
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        "icon".to_string()
    } else {
        name
    }
}

fn write_icon_png(path: &Path, width: usize, height: usize, rgba: &[u8]) -> Option<()> {
    let file = File::create(path).ok()?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(rgba).ok()?;
    Some(())
}

/// 内置图标优先从 `uixSvgIcon_equipment_*` 对应的
/// `uixTxrIcon_gear_*` Texture2D 读取；首次读取后缓存在 exe 同目录。
fn mesh_to_svg(mesh: &unityfs::UnityValue) -> Option<String> {
    let vertex_data = mesh.get("m_VertexData")?;
    let data = match vertex_data.get("m_DataSize") {
        Some(unityfs::UnityValue::Bytes(bytes)) => bytes,
        _ => return None,
    };
    let vertex_count = vertex_data
        .get("m_VertexCount")
        .and_then(|value| value.as_i64())
        .unwrap_or(0) as usize;
    if vertex_count == 0 || data.is_empty() {
        return None;
    }
    let stride = data.len() / vertex_count;
    if stride < 12 {
        return None;
    }
    let mut position_offset = None;
    if let Some(unityfs::UnityValue::Array(channels)) = vertex_data.get("m_Channels") {
        for channel in channels {
            let dimension = channel
                .get("dimension")
                .and_then(|value| value.as_i64())
                .unwrap_or(0);
            let format = channel
                .get("format")
                .and_then(|value| value.as_i64())
                .unwrap_or(-1);
            if dimension == 3 && format == 0 {
                position_offset = channel
                    .get("offset")
                    .and_then(|value| value.as_i64())
                    .map(|value| value as usize);
                break;
            }
        }
    }
    let position_offset = position_offset?;
    let mut vertices = Vec::with_capacity(vertex_count);
    for index in 0..vertex_count {
        let base = index * stride + position_offset;
        if base + 12 > data.len() {
            return None;
        }
        let x = f32::from_le_bytes(data[base..base + 4].try_into().ok()?);
        let y = f32::from_le_bytes(data[base + 4..base + 8].try_into().ok()?);
        vertices.push((x as f64, -(y as f64)));
    }

    let index_bytes: Vec<u8> = match mesh.get("m_IndexBuffer") {
        Some(unityfs::UnityValue::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_i64())
            .map(|value| value as u8)
            .collect(),
        Some(unityfs::UnityValue::Bytes(bytes)) => bytes.clone(),
        _ => return None,
    };
    let index_format = mesh
        .get("m_IndexFormat")
        .and_then(|value| value.as_i64())
        .unwrap_or(0);
    let mut triangles = Vec::new();
    let Some(unityfs::UnityValue::Array(submeshes)) = mesh.get("m_SubMeshes") else {
        return None;
    };
    for submesh in submeshes {
        let first_byte = submesh
            .get("firstByte")
            .and_then(|value| value.as_i64())
            .unwrap_or(0) as usize;
        let index_count = submesh
            .get("indexCount")
            .and_then(|value| value.as_i64())
            .unwrap_or(0) as usize;
        let base_vertex = submesh
            .get("baseVertex")
            .and_then(|value| value.as_i64())
            .unwrap_or(0) as usize;
        let index_size = if index_format == 1 { 4 } else { 2 };
        let start = first_byte;
        for offset in (start..start.saturating_add(index_count * index_size))
            .step_by(index_size * 3)
        {
            if offset + index_size * 3 > index_bytes.len() {
                break;
            }
            let read_index = |at: usize| -> Option<usize> {
                if index_format == 1 {
                    Some(
                        u32::from_le_bytes(index_bytes[at..at + 4].try_into().ok()?) as usize,
                    )
                } else {
                    Some(
                        u16::from_le_bytes(index_bytes[at..at + 2].try_into().ok()?) as usize,
                    )
                }
            };
            let Some(a) = read_index(offset).map(|index| index + base_vertex) else {
                break;
            };
            let Some(b) = read_index(offset + index_size).map(|index| index + base_vertex) else {
                break;
            };
            let Some(c) = read_index(offset + index_size * 2).map(|index| index + base_vertex) else {
                break;
            };
            if a < vertices.len() && b < vertices.len() && c < vertices.len() {
                triangles.push([a, b, c]);
            }
        }
    }
    if triangles.is_empty() {
        return None;
    }

    let min_x = vertices.iter().map(|point| point.0).fold(f64::INFINITY, f64::min);
    let max_x = vertices.iter().map(|point| point.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = vertices.iter().map(|point| point.1).fold(f64::INFINITY, f64::min);
    let max_y = vertices.iter().map(|point| point.1).fold(f64::NEG_INFINITY, f64::max);
    let width = (max_x - min_x).max(0.001);
    let height = (max_y - min_y).max(0.001);

    let mut path = String::new();
    for triangle in &triangles {
        for (point_index, vertex_index) in triangle.iter().enumerate() {
            let (x, y) = vertices[*vertex_index];
            if point_index == 0 {
                path.push('M');
            } else {
                path.push('L');
            }
            path.push_str(&format!("{:.3} {:.3} ", x, y));
        }
        path.push('Z');
    }

    Some(format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.3} {:.3} {:.3} {:.3}"><path fill="#fff" d="{}"/></svg>"##,
        min_x, min_y, width, height, path
    ))
}

/// 内置图标优先从同名 Mesh 生成白色 SVG；前端负责着色。
/// 如果 Mesh 不可用，则回退到 `uixTxrIcon_gear_*` Texture2D。
pub fn resolve_builtin_icons(root: &str, icons: &[String]) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let Some(cache_dir) = icon_cache_dir() else {
        return result;
    };

    let mut missing = Vec::new();
    for icon in icons {
        let svg_path = cache_dir.join(format!("{}.svg", safe_icon_file_name(icon)));
        if svg_path.is_file() {
            result.insert(icon.clone(), svg_path.to_string_lossy().to_string());
        } else {
            // 旧的灰度 PNG 只作为 Mesh SVG 失败后的回退，不能抢先命中。
            missing.push(icon.clone());
        }
    }
    if missing.is_empty() {
        return result;
    }

    let assets_path = Path::new(root)
        .join("BattleTech_Data")
        .join("resources.assets");
    let Ok(bytes) = std::fs::read(&assets_path) else {
        return result;
    };
    let mut reader = unityfs::Reader {
        data: unityfs::reader::ByteSource::Heap(bytes.into()),
        pos: 0,
        endian: unityfs::reader::Endian::Big,
        unity_version: unityfs::UnityVersion::default(),
    };
    let serialized = unityfs::SerializedFile::read(&mut reader);
    let mut manager = unityfs::assets::AssetManager::new();
    manager.add_file("resources.assets".to_string(), serialized);
    let Some(file_name) = manager.files.keys().next().cloned() else {
        return result;
    };
    let Some(serialized) = manager.files.get(&file_name) else {
        return result;
    };

    for object in &serialized.objects {
        if object.class_id != 43 {
            continue;
        }
        let Ok(value) = manager.read_object_value(&file_name, 0, object.path_id) else {
            continue;
        };
        let mesh_name = value
            .get("m_Name")
            .and_then(|field| field.as_str())
            .unwrap_or("");
        if !missing.iter().any(|icon| icon == mesh_name) || result.contains_key(mesh_name) {
            continue;
        }
        let Some(svg) = mesh_to_svg(&value) else {
            continue;
        };
        let out_path = cache_dir.join(format!("{}.svg", safe_icon_file_name(mesh_name)));
        if std::fs::write(&out_path, svg).is_err() {
            continue;
        }
        result.insert(mesh_name.to_string(), out_path.to_string_lossy().to_string());
    }

    for icon in &missing {
        if result.contains_key(icon) {
            continue;
        }
        let png_path = cache_dir.join(format!("{}.png", safe_icon_file_name(icon)));
        if png_path.is_file() {
            result.insert(icon.clone(), png_path.to_string_lossy().to_string());
        }
    }
    missing.retain(|icon| !result.contains_key(icon) && builtin_texture_name(icon).is_some());
    if missing.is_empty() {
        return result;
    }

    let wanted: HashMap<String, String> = missing
        .iter()
        .filter_map(|icon| builtin_texture_name(icon).map(|texture| (texture, icon.clone())))
        .collect();
    let resource_path = assets_path.with_extension("assets.resS");
    let mut resource_file = File::open(&resource_path).ok();

    for object in &serialized.objects {
        if object.class_id != 28 {
            continue;
        }
        let Ok(value) = manager.read_object_value(&file_name, 0, object.path_id) else {
            continue;
        };
        let texture_name = value
            .get("m_Name")
            .and_then(|field| field.as_str())
            .unwrap_or("");
        let Some(icon) = wanted.get(texture_name) else {
            continue;
        };
        if result.contains_key(icon) {
            continue;
        }
        let width = value
            .get("m_Width")
            .and_then(|field| field.as_i64())
            .unwrap_or(0) as usize;
        let height = value
            .get("m_Height")
            .and_then(|field| field.as_i64())
            .unwrap_or(0) as usize;
        let format = value
            .get("m_TextureFormat")
            .and_then(|field| field.as_i64())
            .unwrap_or(0) as i32;
        if width == 0 || height == 0 {
            continue;
        }
        let mut data = match value.get("image_data") {
            Some(unityfs::UnityValue::Bytes(bytes)) => bytes.clone(),
            Some(unityfs::UnityValue::String(text)) => text.as_bytes().to_vec(),
            _ => Vec::new(),
        };
        if data.is_empty() {
            if let Some(stream) = value.get("m_StreamData") {
                let offset = stream
                    .get("offset")
                    .and_then(|field| field.as_i64())
                    .unwrap_or(0) as u64;
                let size = stream
                    .get("size")
                    .and_then(|field| field.as_i64())
                    .unwrap_or(0) as usize;
                if let Some(file) = resource_file.as_mut() {
                    let mut buffer = vec![0u8; size];
                    if file.seek(SeekFrom::Start(offset)).is_ok()
                        && file.read_exact(&mut buffer).is_ok()
                    {
                        data = buffer;
                    }
                }
            }
        }
        if data.is_empty() {
            continue;
        }
        let Some(rgba) = unityfs::decompress_texture(width, height, format, &data) else {
            continue;
        };
        let out_path = cache_dir.join(format!("{}.png", safe_icon_file_name(icon)));
        if write_icon_png(&out_path, width, height, &rgba).is_none() {
            continue;
        }
        result.insert(icon.clone(), out_path.to_string_lossy().to_string());
    }

    result
}

fn load_color_metadata(
    root: &str,
) -> (
    HashMap<String, String>,
    HashMap<String, String>,
    HashMap<String, i64>,
    std::collections::HashSet<String>,
) {
    let mut colors = HashMap::new();
    let mut categories = HashMap::new();
    let mut sorters = HashMap::new();
    let mut background_categories = std::collections::HashSet::new();
    let custom_components = Path::new(root)
        .join("Mods")
        .join("Core")
        .join("CustomComponents")
        .join("mod.json");
    if let Some(value) = read_json(&custom_components) {
        if let Some(tags) = value
            .pointer("/Settings/ColorTags")
            .and_then(Value::as_array)
        {
            for entry in tags {
                let tag = entry.get("Tag").and_then(Value::as_str).unwrap_or("");
                let color = entry.get("Color").and_then(Value::as_str).unwrap_or("");
                if !tag.is_empty() && !color.is_empty() {
                    colors.insert(tag.to_string(), color.to_string());
                }
            }
        }
    }

    let ui_colors = Path::new(root)
        .join("BattleTech_Data")
        .join("StreamingAssets")
        .join("data")
        .join("ui")
        .join("UIColorsDefault.json");
    if let Some(value) = read_json(&ui_colors) {
        if let Some(entries) = value.as_object() {
            for (name, color) in entries {
                if let Some(color) = color.as_str() {
                    colors.insert(name.to_ascii_lowercase(), color.to_string());
                }
            }
        }
    }

    let category_dir = Path::new(root)
        .join("Mods")
        .join("Core")
        .join("RogueTechCore")
        .join("categories");
    if let Ok(entries) = std::fs::read_dir(category_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let Some(value) = read_json(&entry.path()) else {
                continue;
            };
            let Some(settings) = value.get("Settings").and_then(Value::as_array) else {
                continue;
            };
            for setting in settings {
                let name = setting.get("Name").and_then(Value::as_str).unwrap_or("");
                let tag = setting
                    .pointer("/DefaultCustoms/ColorTag")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !name.is_empty() && !tag.is_empty() {
                    categories.insert(name.to_string(), tag.to_string());
                    background_categories.insert(name.to_string());
                }
                if let Some(sorter) = setting
                    .pointer("/DefaultCustoms/Sorter")
                    .and_then(Value::as_i64)
                {
                    sorters.insert(name.to_string(), sorter);
                }
                if let Some(color) = setting
                    .pointer("/DefaultCustoms/TRGBColor/Color")
                    .and_then(Value::as_str)
                    .filter(|color| !color.is_empty())
                {
                    colors
                        .entry(name.to_string())
                        .or_insert_with(|| color.to_string());
                    categories
                        .entry(name.to_string())
                        .or_insert_with(|| name.to_string());
                }
                if let Some(rgb) = setting
                    .pointer("/DefaultCustoms/RGBColor")
                    .and_then(Value::as_str)
                    .filter(|color| !color.is_empty())
                {
                    categories
                        .entry(name.to_string())
                        .or_insert_with(|| name.to_string());
                    colors.insert(name.to_ascii_lowercase(), rgb.to_string());
                    background_categories.insert(name.to_string());
                }
                if let Some(color_name) = setting
                    .pointer("/DefaultCustoms/Color")
                    .and_then(Value::as_str)
                    .filter(|color| colors.contains_key(&color.to_ascii_lowercase()))
                {
                    categories
                        .entry(name.to_string())
                        .or_insert_with(|| color_name.to_string());
                    if let Some(hex) = colors.get(&color_name.to_ascii_lowercase()).cloned() {
                        colors.insert(name.to_ascii_lowercase(), hex);
                    }
                    background_categories.insert(name.to_string());
                }
            }
        }
    }

    // EngineSlots 本身没有颜色，Linked Engine 使用游戏绿色背景，
    // 与 Engine XL 的 EngineShield 蓝色区分。
    categories
        .entry("EngineSlots".to_string())
        .or_insert_with(|| "Green".to_string());

    (colors, categories, sorters, background_categories)
}

/// 优先读取 exe 同目录下的缓存；仅在缓存不存在、版本不符或
/// `roguelauncherData.ibin` 修改时间变化时重新扫描 Mods。
pub fn scan_or_load(root: &str) -> Result<DataCache, String> {
    let stamp = mods_stamp(root);
    if let Some(cache) = read_cache(root, stamp) {
        return Ok(cache);
    }

    let cache = scan(root)?;
    write_cache(&cache, stamp);
    Ok(cache)
}

pub fn scan(root: &str) -> Result<DataCache, String> {
    let mods = Path::new(root).join("Mods");
    if !mods.is_dir() {
        return Err(format!("未找到 Mods 目录: {}", mods.display()));
    }

    let weapon_re = Regex::new(
        r"(?i)^(?:Unique_BoltOn_|Unique_HandHeld_|Unique_|BoltOn_|HandHeld_)?Weapon_.*\.json$",
    )
    .map_err(|e| e.to_string())?;

    let mut entries: BTreeMap<String, DataEntry> = BTreeMap::new();
    let mut chassis_paths = HashMap::new();
    let mut vehicle_chassis_paths = HashMap::new();
    let mut component_paths = HashMap::new();
    let mut icon_paths = HashMap::new();
    let mut affinities = Vec::new();
    let mut mech_defaults = None;

    let walker = WalkDir::new(&mods)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !skip_dir(e));

    for item in walker.filter_map(Result::ok) {
        if !item.file_type().is_file() {
            continue;
        }
        let path = item.path();
        let file_name = item.file_name().to_string_lossy().to_string();
        let lower = file_name.to_ascii_lowercase();
        if lower.ends_with(".svg") {
            icon_paths
                .entry(file_stem(path).to_ascii_lowercase())
                .or_insert_with(|| path.to_path_buf());
            continue;
        }
        if !lower.ends_with(".json") {
            continue;
        }

        // 底盘文件只需要路径映射，列表阶段不解析内容，避免扫描 7000+ 个底盘。
        if lower.starts_with("chassisdef_") {
            insert_path(&mut chassis_paths, "", path);
            continue;
        }
        if lower.starts_with("vehiclechassisdef_") {
            insert_path(&mut vehicle_chassis_paths, "", path);
            continue;
        }

        let Some(value) = read_json(path) else {
            continue;
        };
        let id = def_id(&value);

        if lower == "defaults_mechengineer.json" {
            mech_defaults = Some(value);
            continue;
        }

        if lower.starts_with("mechdef_") {
            let key = if id.is_empty() { file_stem(path) } else { id };
            let name = display_name(&value, &key);
            let subtitle = pointer_str(&value, &["Description", "Name"]).to_string();
            let chassis_id = value
                .get("ChassisID")
                .and_then(Value::as_str)
                .unwrap_or("");
            let plain_name = plain_text(&name);
            let search_text = format!("{plain_name} {chassis_id} {}", plain_text(&subtitle));
            entries.insert(
                format!("mech:{key}"),
                DataEntry {
                    id: key,
                    kind: "mech".into(),
                    name,
                    subtitle,
                    plain_text: plain_name,
                    search_text,
                    path: path.to_string_lossy().to_string(),
                },
            );
            continue;
        }

        if lower.starts_with("vehicledef_") {
            let key = if id.is_empty() { file_stem(path) } else { id };
            let name = display_name(&value, &key);
            let subtitle = pointer_str(&value, &["Description", "Name"]).to_string();
            let chassis_id = value
                .get("ChassisID")
                .and_then(Value::as_str)
                .unwrap_or("");
            let plain_name = plain_text(&name);
            let search_text = format!("{plain_name} {chassis_id} {}", plain_text(&subtitle));
            entries.insert(
                format!("vehicle:{key}"),
                DataEntry {
                    id: key,
                    kind: "vehicle".into(),
                    name,
                    subtitle,
                    plain_text: plain_name,
                    search_text,
                    path: path.to_string_lossy().to_string(),
                },
            );
            continue;
        }

        if lower.starts_with("affinitydef_") {
            affinities.push(value);
            continue;
        }

        if weapon_re.is_match(&file_name) {
            if !is_component(&value) {
                continue;
            }
            let key = if id.is_empty() { file_stem(path) } else { id };
            let name = display_name(&value, &key);
            let subtitle = [
                pointer_str(&value, &["Category"]),
                pointer_str(&value, &["Type"]),
            ]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" / ");
            let plain_name = plain_text(&name);
            entries.insert(
                format!("weapon:{key}"),
                DataEntry {
                    id: key.clone(),
                    kind: "weapon".into(),
                    name: name.clone(),
                    subtitle,
                    plain_text: plain_name.clone(),
                    search_text: plain_name,
                    path: path.to_string_lossy().to_string(),
                },
            );
            insert_path(&mut component_paths, &key, path);
            continue;
        }

        if component_prefix(&lower) && is_component(&value) {
            let key = if id.is_empty() { file_stem(path) } else { id };
            let name = display_name(&value, &key);
            let subtitle = pointer_str(&value, &["ComponentType"]).to_string();
            let kind = if subtitle.eq_ignore_ascii_case("AmmunitionBox")
                || lower.starts_with("ammo_")
                || lower.contains("ammunition")
            {
                "ammo"
            } else {
                "equipment"
            };
            let plain_name = plain_text(&name);
            entries.insert(
                format!("{kind}:{key}"),
                DataEntry {
                    id: key.clone(),
                    kind: kind.into(),
                    name: name.clone(),
                    subtitle,
                    plain_text: plain_name.clone(),
                    search_text: plain_name,
                    path: path.to_string_lossy().to_string(),
                },
            );
            insert_path(&mut component_paths, &key, path);
        }
    }

    let mut entries: Vec<DataEntry> = entries.into_values().collect();
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let (color_tags, category_color_tags, category_sorters, category_background_tags) =
        load_color_metadata(root);
    Ok(DataCache {
        root: root.to_string(),
        entries,
        chassis_paths,
        vehicle_chassis_paths,
        component_paths,
        icon_paths,
        color_tags,
        category_color_tags,
        category_background_tags,
        category_sorters,
        affinities,
        mech_defaults,
    })
}

fn component_value(
    cache: &DataCache,
    cache_map: &mut HashMap<String, Value>,
    id: &str,
) -> Option<Value> {
    if !cache_map.contains_key(id) {
        let path = cache.component_paths.get(id)?;
        let value = read_json(path)?;
        cache_map.insert(id.to_string(), value);
    }
    cache_map.get(id).cloned()
}

fn actuator_icon_entry(
    cache: &DataCache,
    cache_map: &mut HashMap<String, Value>,
    id: &str,
) -> Value {
    let Some(value) = component_value(cache, cache_map, id) else {
        return json!({ "icon": "", "path": Value::Null });
    };
    let icon = pointer_str(&value, &["Description", "Icon"]);
    let path = cache
        .icon_paths
        .get(&icon.to_ascii_lowercase())
        .map(|path| path.to_string_lossy().to_string());
    let color_tag = resolve_color_tag(cache, &value);
    json!({ "icon": icon, "path": path, "colorTag": color_tag })
}

fn component_slots(value: &Value) -> i64 {
    let base = as_i64(value, "InventorySize").max(1);
    // 带 Linked 的部件由左右躯等位置上的 Linked_* 部件实际占位，
    // 例如 Engine XL 本体只占安装位置的 2 槽，左右躯各由 3 槽的 Linked_Engine_Size_3 占位。
    let has_linked = value
        .pointer("/Custom/Linked/Links")
        .and_then(Value::as_array)
        .map(|links| !links.is_empty())
        .unwrap_or(false);
    if has_linked {
        return base;
    }
    let dynamic = component_dynamic_slots(value);
    // MechEngineer 会把超过 7 槽的武器拆成 7 槽本体 + 可搬移动态槽。
    if pointer_str(value, &["ComponentType"]).eq_ignore_ascii_case("weapon") && base > 7 {
        return (base - dynamic).max(1);
    }
    let ibls = value
        .pointer("/Custom/IBLS/StorageSize")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    // IBLS 是基础槽 + 动态槽的总数；安装位置只占加号前面的基础槽。
    let implied_base = (ibls - dynamic).max(1);
    base.max(implied_base).max(1)
}

fn component_dynamic_slots(value: &Value) -> i64 {
    // 只有 DynamicSlots 是真正需要被挤到其他部位的空槽；
    // Weights.ReservedSlots 若由 Linked_* 部件表示，就不再重复分配。
    let explicit = value
        .pointer("/Custom/DynamicSlots/ReservedSlots")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if explicit > 0 {
        return explicit;
    }
    let inventory_size = as_i64(value, "InventorySize");
    if pointer_str(value, &["ComponentType"]).eq_ignore_ascii_case("weapon")
        && inventory_size > 7
    {
        return inventory_size - 7;
    }
    explicit
}

fn resolve_color_tag(cache: &DataCache, definition: &Value) -> String {
    if let Some(tag) = definition
        .pointer("/Custom/ColorTag")
        .and_then(Value::as_str)
        .filter(|tag| !tag.is_empty())
    {
        return tag.to_string();
    }
    if let Some(color_name) = definition
        .pointer("/Custom/Color")
        .and_then(Value::as_str)
        .filter(|color| cache.color_tags.contains_key(&color.to_ascii_lowercase()))
    {
        return color_name.to_string();
    }
    let Some(categories) = definition
        .pointer("/Custom/Category")
        .and_then(Value::as_array)
    else {
        return String::new();
    };
    let mut category_ids: Vec<&str> = categories
        .iter()
        .filter_map(|category| category.get("CategoryID").and_then(Value::as_str))
        .collect();
    category_ids.sort_by_key(|id| match id.to_ascii_lowercase().as_str() {
        "special" => 0,
        "handheld" => 1,
        "addon" => 2,
        _ => 3,
    });
    for id in category_ids {
        if let Some(tag) = cache.category_color_tags.get(id) {
            return tag.clone();
        }
        if cache.color_tags.contains_key(id) {
            return id.to_string();
        }
    }
    String::new()
}

fn resolve_background_tag(cache: &DataCache, definition: &Value) -> String {
    if let Some(tag) = definition
        .pointer("/Custom/ColorTag")
        .and_then(Value::as_str)
        .filter(|tag| !tag.is_empty())
    {
        return tag.to_string();
    }
    if let Some(color_name) = definition
        .pointer("/Custom/Color")
        .and_then(Value::as_str)
        .filter(|color| cache.color_tags.contains_key(&color.to_ascii_lowercase()))
    {
        return color_name.to_string();
    }
    let Some(categories) = definition
        .pointer("/Custom/Category")
        .and_then(Value::as_array)
    else {
        return String::new();
    };
    let mut category_ids: Vec<&str> = categories
        .iter()
        .filter_map(|category| category.get("CategoryID").and_then(Value::as_str))
        .collect();
    category_ids.sort_by_key(|id| match id.to_ascii_lowercase().as_str() {
        "special" => 0,
        "handheld" => 1,
        "addon" => 2,
        _ => 3,
    });
    for id in category_ids {
        if !cache.category_background_tags.contains(id) {
            continue;
        }
        if let Some(tag) = cache.category_color_tags.get(id) {
            return tag.clone();
        }
    }
    String::new()
}

fn item_view(
    cache: &DataCache,
    cache_map: &mut HashMap<String, Value>,
    item: &Value,
) -> Value {
    let id = item
        .get("ComponentDefID")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let def = component_value(cache, cache_map, &id);
    let name = def
        .as_ref()
        .map(|d| display_name(d, &id))
        .unwrap_or_else(|| id.clone());
    let mut ctype = item
        .get("ComponentDefType")
        .and_then(Value::as_str)
        .or_else(|| {
            def.as_ref()
                .and_then(|d| d.get("ComponentType"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .to_string();
    if id.to_ascii_lowercase().starts_with("linked_") {
        ctype = "Linked".to_string();
    }
    let slots = def.as_ref().map(component_slots).unwrap_or(1);
    let dynamic_slots = def
        .as_ref()
        .map(component_dynamic_slots)
        .unwrap_or(0);
    let tonnage = def.as_ref().map(|d| as_f64(d, "Tonnage")).unwrap_or(0.0);
    let category = def
        .as_ref()
        .and_then(|d| d.get("Category"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let weapon_category = def
        .as_ref()
        .and_then(|d| d.get("weaponCategoryID"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let tags = def
        .as_ref()
        .and_then(|d| d.pointer("/ComponentTags/items"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let custom_categories = def
        .as_ref()
        .and_then(|d| d.pointer("/Custom/Category"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("CategoryID").and_then(Value::as_str))
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    let id_lower = id.to_ascii_lowercase();
    let weapon_attachment = id_lower.contains("attachment")
        || custom_categories.contains("weaponattachment");
    let equipment_color =
        ctype == "Linked" && custom_categories.contains("engineslots");
    let text_color = def
        .as_ref()
        .and_then(|definition| definition.pointer("/Custom/TRGBColor/Color"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let background_tag = def
        .as_ref()
        .map(|definition| resolve_background_tag(cache, definition))
        .unwrap_or_default();
    let background_color = cache
        .color_tags
        .get(&background_tag.to_ascii_lowercase())
        .cloned()
        .unwrap_or_default();
    let hardpoint_type = if id_lower.contains("handheld")
        || tags.contains("handheld")
        || custom_categories.contains("handheld")
    {
        "specialhandheld"
    } else if custom_categories
        .split_whitespace()
        .any(|value| value == "lamwingmount")
    {
        "wingmountedweapon"
    } else if custom_categories
        .split_whitespace()
        .any(|value| value == "laminternalbombbay")
    {
        "internalbombbay"
    } else if id_lower.contains("bolton")
        || tags.contains("bolton")
        || weapon_category.eq_ignore_ascii_case("special")
        || custom_categories.split_whitespace().any(|value| value == "special")
    {
        "special"
    } else {
        match category.to_ascii_lowercase().as_str() {
            "ballistic" => "ballistic",
            "energy" => "energy",
            "missile" => "missile",
            "antipersonnel" => "antipersonnel",
            _ => "",
        }
    };
    let details = def
        .as_ref()
        .map(|d| pointer_str(d, &["Description", "Details"]))
        .unwrap_or("");
    let linked: Vec<String> = def
        .as_ref()
        .and_then(|d| d.pointer("/Custom/Linked/Links"))
        .and_then(Value::as_array)
        .map(|links| {
            links
                .iter()
                .filter_map(|link| link.get("Location").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let icon_key = def
        .as_ref()
        .map(|d| pointer_str(d, &["Description", "Icon"]))
        .unwrap_or("")
        .to_string();
    let icon_path = cache
        .icon_paths
        .get(&icon_key.to_ascii_lowercase())
        .map(|path| path.to_string_lossy().to_string());
    let sorter = def
        .as_ref()
        .and_then(|definition| definition.pointer("/Custom/Sorter"))
        .and_then(Value::as_i64)
        .or_else(|| {
            def.as_ref()
                .and_then(|definition| definition.pointer("/Custom/Category"))
                .and_then(Value::as_array)
                .and_then(|categories| {
                    categories
                        .iter()
                        .filter_map(|category| {
                            category.get("CategoryID").and_then(Value::as_str)
                        })
                        .find_map(|id| cache.category_sorters.get(id).copied())
                })
        })
        .unwrap_or(0);
    let sorter = if id.to_ascii_lowercase().starts_with("linked_engine") {
        cache
            .category_sorters
            .get("EngineShield")
            .copied()
            .unwrap_or(sorter)
    } else {
        sorter
    };
    let dynamic_config = def
        .as_ref()
        .and_then(|definition| definition.pointer("/Custom/DynamicSlots"));
    let dynamic_show_icon = dynamic_config
        .and_then(|config| config.get("ShowIcon"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let dynamic_name = dynamic_config
        .and_then(|config| config.get("NameText"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let dynamic_bonus = dynamic_config
        .and_then(|config| config.get("BonusAText"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let dynamic_color_tag = dynamic_config
        .and_then(|config| config.get("BackgroundColor"))
        .and_then(Value::as_str)
        .filter(|color| cache.color_tags.contains_key(&color.to_ascii_lowercase()))
        .unwrap_or("");
    let raw_inventory_size = def
        .as_ref()
        .map(|definition| as_i64(definition, "InventorySize").max(1))
        .unwrap_or(1);
    let weapon_split = ctype.eq_ignore_ascii_case("Weapon") && raw_inventory_size > 7;
    let dynamic_inner_adjacent = dynamic_config
        .and_then(|config| config.get("InnerAdjacentOnly"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || weapon_split;
    let inventory_size = if weapon_split {
        7
    } else {
        raw_inventory_size
    };
    let dynamic_bonus = if dynamic_bonus.eq_ignore_ascii_case("dynamic slot") {
        "movable slot"
    } else {
        dynamic_bonus
    };
    let color_tag = def
        .as_ref()
        .map(|definition| resolve_color_tag(cache, definition))
        .unwrap_or_default();
    let fixed_overlay = ctype != "Linked"
        && (item
            .get("IsFixed")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || def
                .as_ref()
                .and_then(|definition| definition.pointer("/Custom/Flags"))
                .and_then(Value::as_array)
                .map(|flags| {
                    flags.iter().filter_map(Value::as_str).any(|flag| {
                        flag.eq_ignore_ascii_case("default")
                            || flag.eq_ignore_ascii_case("preinstalled")
                    })
                })
                .unwrap_or(false));

    json!({
        "id": id,
        "name": name,
        "icon": icon_key,
        "iconPath": icon_path,
        "componentType": ctype,
        "category": category,
        "hardpointType": hardpoint_type,
        "customCategories": custom_categories
            .split_whitespace()
            .collect::<Vec<_>>(),
        "weaponAttachment": weapon_attachment,
        "equipment": equipment_color,
        "textColor": text_color,
        "slots": slots,
        "dynamicSlots": dynamic_slots,
        "dynamicShowIcon": dynamic_show_icon,
        "dynamicName": dynamic_name,
        "dynamicBonus": dynamic_bonus,
        "dynamicColorTag": dynamic_color_tag,
        "dynamicInnerAdjacent": dynamic_inner_adjacent,
        "inventorySize": inventory_size,
        "colorTag": color_tag,
        "backgroundTag": background_tag,
        "backgroundColor": background_color,
        "preinstalled": fixed_overlay,
        "sorter": sorter,
        "tonnage": tonnage,
        "location": item.get("MountedLocation").and_then(Value::as_str).unwrap_or(""),
        "hardpointSlot": item.get("HardpointSlot").and_then(Value::as_i64).unwrap_or(-1),
        "fixed": item.get("IsFixed").and_then(Value::as_bool).unwrap_or(false),
        "linked": linked,
        "details": details,
    })
}

fn hardpoint_label(key: &str) -> &str {
    match key {
        "ballistic" => "弹道",
        "energy" => "能量",
        "missile" => "导弹",
        "universal" => "万能",
        "antipersonnel" => "支援",
        "special" => "特殊",
        "specialhandheld" => "手持",
        "wingmountedweapon" => "翼挂",
        "internalbombbay" => "弹舱",
        _ => key,
    }
}

fn hardpoint_key(mount: &str) -> &'static str {
    match mount.to_ascii_lowercase().as_str() {
        "ballistic" => "ballistic",
        "energy" => "energy",
        "missile" => "missile",
        "omni" | "universal" => "universal",
        "battlearmor" => "antipersonnel",
        "antipersonnel" => "antipersonnel",
        "special" => "special",
        "specialhandheld" => "specialhandheld",
        "wingmountedweapon" => "wingmountedweapon",
        "internalbombbay" => "internalbombbay",
        _ => "special",
    }
}

fn weapon_hardpoint(def: &Value) -> &'static str {
    let is_battle_armor_weapon = def
        .pointer("/ComponentTags/items")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .any(|tag| tag.eq_ignore_ascii_case("BattleArmorWeapon"))
        })
        .unwrap_or(false);
    let is_support_category = def
        .pointer("/Custom/Category")
        .and_then(Value::as_array)
        .map(|categories| {
            categories.iter().any(|category| {
                category
                    .get("CategoryID")
                    .and_then(Value::as_str)
                    .map(|id| id.to_ascii_lowercase().starts_with("w/s/"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    if is_battle_armor_weapon || is_support_category {
        return "antipersonnel";
    }
    match pointer_str(def, &["Category"]).to_ascii_lowercase().as_str() {
        "ballistic" => "ballistic",
        "energy" => "energy",
        "missile" => "missile",
        "antipersonnel" => "antipersonnel",
        _ => "special",
    }
}

fn hardpoint_stats(
    chassis_loc: &Value,
    items: &[Value],
) -> Vec<Value> {
    let mut max: HashMap<&'static str, i64> = HashMap::new();
    let mut has_omni = false;
    if let Some(hardpoints) = chassis_loc.get("Hardpoints").and_then(Value::as_array) {
        for hp in hardpoints {
            let omni = hp.get("Omni").and_then(Value::as_bool).unwrap_or(false);
            has_omni |= omni;
            let key = if omni {
                "universal"
            } else {
                hardpoint_key(
                    hp.get("WeaponMountID")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                )
            };
            *max.entry(key).or_insert(0) += 1;
        }
    }
    let mut used: HashMap<&'static str, i64> = HashMap::new();
    for item in items {
        let key = item
            .get("hardpointType")
            .and_then(Value::as_str)
            .unwrap_or("");
        if key.is_empty() {
            if item.get("componentType").and_then(Value::as_str) != Some("Weapon") {
                continue;
            }
            let category = item
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("");
            let fallback = match category.to_ascii_lowercase().as_str() {
                "ballistic" => "ballistic",
                "energy" => "energy",
                "missile" => "missile",
                "antipersonnel" => "antipersonnel",
                _ => continue,
            };
            let fallback = if has_omni
                && matches!(fallback, "ballistic" | "energy" | "missile" | "antipersonnel")
            {
                "universal"
            } else {
                fallback
            };
            *used.entry(fallback).or_insert(0) += 1;
            continue;
        }
        let key = match key {
            "special" => "special",
            "specialhandheld" => "specialhandheld",
            "ballistic" => "ballistic",
            "energy" => "energy",
            "missile" => "missile",
            "antipersonnel" => "antipersonnel",
            "wingmountedweapon" => "wingmountedweapon",
            "internalbombbay" => "internalbombbay",
            _ => continue,
        };
        let key = if has_omni
            && matches!(key, "ballistic" | "energy" | "missile" | "antipersonnel")
        {
            "universal"
        } else {
            key
        };
        *used.entry(key).or_insert(0) += 1;
    }
    let mut keys: Vec<&'static str> = max.keys().copied().collect();
    for key in used.keys() {
        if !keys.contains(key) {
            keys.push(key);
        }
    }
    keys.sort_by_key(|key| match *key {
        "ballistic" => 0,
        "energy" => 1,
        "missile" => 2,
        "universal" => 3,
        "specialhandheld" => 4,
        "special" => 5,
        "wingmountedweapon" => 6,
        "internalbombbay" => 7,
        "antipersonnel" => 9,
        _ => 8,
    });
    keys.into_iter()
        .map(|key| {
            json!({
                "key": key,
                "label": hardpoint_label(key),
                "used": used.get(key).copied().unwrap_or(0),
                "max": max.get(key).copied().unwrap_or(0),
            })
        })
        .collect()
}

fn is_chassis_level_component(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    lower.starts_with("gear_armor_")
        || lower.starts_with("default_armor_")
        || lower.starts_with("unique_armor_")
        || lower.starts_with("unique_gear_armor_")
        || lower.starts_with("gear_structure_")
        || lower.starts_with("default_structure_")
        || lower.starts_with("unique_structure_")
        || lower.starts_with("unique_gear_structure_")
        || lower.starts_with("gear_superheavyframe_")
        || lower.starts_with("gear_mechtrait_")
}

fn chassis_location<'a>(chassis: &'a Value, name: &str) -> Option<&'a Value> {
    chassis
        .get("Locations")
        .and_then(Value::as_array)
        .and_then(|locs| {
            locs.iter().find(|loc| {
                loc.get("Location").and_then(Value::as_str) == Some(name)
            })
        })
}

fn location_label(name: &str) -> &str {
    match name {
        "Head" => "头部",
        "LeftArm" => "左臂",
        "RightArm" => "右臂",
        "LeftTorso" => "左躯",
        "RightTorso" => "右躯",
        "LeftLeg" => "左腿",
        "RightLeg" => "右腿",
        "CenterTorso" => "中央躯干",
        "Front" => "前部",
        "Left" => "左侧",
        "Right" => "右侧",
        "Rear" => "后部",
        "Turret" => "炮塔",
        _ => name,
    }
}

fn is_case_component(item: &Value) -> bool {
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    let category = item
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    id.contains("case") || category.contains("case")
}

fn is_linked_engine(item: &Value) -> bool {
    item.get("componentType").and_then(Value::as_str) == Some("Linked")
        && item
            .get("customCategories")
            .and_then(Value::as_array)
            .map(|categories| {
                categories
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|category| category.eq_ignore_ascii_case("engineslots"))
            })
            .unwrap_or(false)
}

fn is_lootable_engine_cooling(item: &Value) -> bool {
    item.get("id")
        .and_then(Value::as_str)
        .map(|id| id.to_ascii_lowercase().starts_with("lootable_enginecooling_"))
        .unwrap_or(false)
}

fn is_turret_component(item: &Value) -> bool {
    item.get("customCategories")
        .and_then(Value::as_array)
        .map(|categories| {
            categories
                .iter()
                .filter_map(Value::as_str)
                .any(|category| category.eq_ignore_ascii_case("turret"))
        })
        .unwrap_or(false)
}

fn is_cooling_component(item: &Value) -> bool {
    item.get("customCategories")
        .and_then(Value::as_array)
        .map(|categories| {
            categories
                .iter()
                .filter_map(Value::as_str)
                .any(|category| category.eq_ignore_ascii_case("cooling"))
        })
        .unwrap_or(false)
}

fn equipment_sort_rank(item: &Value) -> i32 {
    let component_type = item
        .get("componentType")
        .and_then(Value::as_str)
        .unwrap_or("");
    let custom_categories = item.get("customCategories").and_then(Value::as_array);
    if custom_categories
        .map(|categories| {
            categories
                .iter()
                .filter_map(Value::as_str)
                .any(|category| category.eq_ignore_ascii_case("heatsink"))
        })
        .unwrap_or(false)
    {
        return 4;
    }
    if component_type == "Weapon" {
        let hardpoint = item
            .get("hardpointType")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        let category = item
            .get("category")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_ascii_lowercase();
        return match if hardpoint.is_empty() {
            category.as_str()
        } else {
            hardpoint.as_str()
        } {
            "ballistic" => 11,
            "energy" => 12,
            "missile" => 13,
            "antipersonnel" => 14,
            _ => 15,
        };
    }
    if is_case_component(item) {
        return 18;
    }
    if component_type.eq_ignore_ascii_case("AmmunitionBox")
        || component_type.eq_ignore_ascii_case("Ammo")
    {
        return 16;
    }
    if item
        .get("customCategories")
        .and_then(Value::as_array)
        .map(|categories| {
            categories
                .iter()
                .filter_map(Value::as_str)
                .any(|category| category.eq_ignore_ascii_case("linkedarmormod"))
        })
        .unwrap_or(false)
    {
        return 17;
    }
    if item
        .get("weaponAttachment")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return 17;
    }
    if is_turret_component(item) {
        return 16;
    }
    10
}

fn default_structure_id(chassis: &Value) -> &'static str {
    let tags = chassis
        .pointer("/ChassisTags/items")
        .and_then(Value::as_array);
    let superheavy = tags
        .map(|tags| {
            tags.iter()
                .filter_map(Value::as_str)
                .any(|tag| tag.to_ascii_lowercase().contains("superheavy"))
        })
        .unwrap_or(false);
    let lam = tags
        .map(|tags| {
            tags.iter().filter_map(Value::as_str).any(|tag| {
                let tag = tag.to_ascii_lowercase();
                tag.contains("landairmech") || tag == "lam"
            })
        })
        .unwrap_or(false);
    if lam {
        if superheavy {
            "Gear_Structure_LAM_Superheavy"
        } else {
            "Default_Structure_LAM_Standard"
        }
    } else if superheavy {
        "Default_Structure_Standard_Superheavy"
    } else {
        "Default_Structure_Standard"
    }
}

fn has_component_category(
    cache: &DataCache,
    comps: &mut HashMap<String, Value>,
    inventory: &[Value],
    chassis: &Value,
    category_name: &str,
) -> bool {
    let mut ids: Vec<&str> = inventory
        .iter()
        .filter_map(|item| item.get("ComponentDefID").and_then(Value::as_str))
        .collect();
    if let Some(fixed) = chassis.get("FixedEquipment").and_then(Value::as_array) {
        ids.extend(
            fixed
                .iter()
                .filter_map(|item| item.get("ComponentDefID").and_then(Value::as_str)),
        );
    }
    ids.into_iter().any(|id| {
        component_value(cache, comps, id)
            .and_then(|value| {
                value
                    .pointer("/Custom/Category")
                    .and_then(Value::as_array)
                    .map(|categories| {
                        categories.iter().any(|category| {
                            category
                                .get("CategoryID")
                                .and_then(Value::as_str)
                                .map(|category| {
                                    category.eq_ignore_ascii_case(category_name)
                                })
                                .unwrap_or(false)
                        })
                    })
            })
            .unwrap_or(false)
    })
}

fn build_locations(
    cache: &DataCache,
    unit: &Value,
    chassis: &Value,
    comps: &mut HashMap<String, Value>,
) -> Vec<Value> {
    let inventory = unit
        .get("inventory")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let locations = unit
        .get("Locations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let multi_defaults = chassis
        .pointer("/Custom/MultiDefaults")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    locations
        .iter()
        .map(|loc| {
            let loc_name = loc
                .get("Location")
                .and_then(Value::as_str)
                .unwrap_or("");
            let chassis_loc = chassis_location(chassis, loc_name).cloned().unwrap_or(Value::Null);
            let inventory_items: Vec<Value> = inventory
                .iter()
                .filter(|item| {
                    item.get("MountedLocation").and_then(Value::as_str) == Some(loc_name)
                })
                .cloned()
                .collect();
            let existing_ids: std::collections::HashSet<String> = inventory_items
                .iter()
                .filter_map(|item| item.get("ComponentDefID").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
            let mut source_items: Vec<Value> = multi_defaults
                .iter()
                .filter(|item| {
                    item.get("Location").and_then(Value::as_str) == Some(loc_name)
                })
                .filter(|item| {
                    !existing_ids.contains(
                        item.get("DefID").and_then(Value::as_str).unwrap_or(""),
                    )
                })
                .map(|item| {
                    json!({
                        "MountedLocation": loc_name,
                        "ComponentDefID": item.get("DefID").and_then(Value::as_str).unwrap_or(""),
                        "ComponentDefType": item.get("ComponentType").and_then(Value::as_str).unwrap_or("Upgrade"),
                        "HardpointSlot": -1,
                        "IsFixed": true,
                    })
                })
                .collect();
            source_items.extend(inventory_items);
            // 装甲/结构属于底盘卡片内容，不重复出现在部位槽位里，
            // 但它们仍占用该部位的容量；这里只记录为 reservedSlots。
            let mut reserved_slots = 0_i64;
            let mut seen_ids = std::collections::HashSet::new();
            for item in &source_items {
                if let Some(id) = item.get("ComponentDefID").and_then(Value::as_str) {
                    seen_ids.insert(id.to_string());
                }
            }
            if let Some(fixed) = chassis.get("FixedEquipment").and_then(Value::as_array) {
                for item in fixed {
                    if item.get("MountedLocation").and_then(Value::as_str) != Some(loc_name) {
                        continue;
                    }
                    let id = item
                        .get("ComponentDefID")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if id.is_empty() || seen_ids.contains(id) {
                        continue;
                    }
                    if is_chassis_level_component(id)
                        || id.to_ascii_lowercase().starts_with("quirk_")
                    {
                        continue;
                    }
                    let mut fixed_item = item.clone();
                    if let Some(object) = fixed_item.as_object_mut() {
                        object.insert("IsFixed".into(), Value::Bool(true));
                    }
                    source_items.push(fixed_item);
                    seen_ids.insert(id.to_string());
                }
            }
            let mut items = Vec::new();
            for view in source_items.iter().map(|item| item_view(cache, comps, item)) {
                let id = view
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if is_lootable_engine_cooling(&view) {
                    seen_ids.insert(id.clone());
                    continue;
                }
                seen_ids.insert(id.clone());
                if is_chassis_level_component(&id) {
                    reserved_slots += view
                        .get("slots")
                        .and_then(Value::as_i64)
                        .unwrap_or(1);
                } else {
                    items.push(view);
                }
            }
            if let Some(fixed) = chassis.get("FixedEquipment").and_then(Value::as_array) {
                for item in fixed {
                    let id = item
                        .get("ComponentDefID")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if id.is_empty() || seen_ids.contains(id) {
                        continue;
                    }
                    if item.get("MountedLocation").and_then(Value::as_str) != Some(loc_name) {
                        continue;
                    }
                    let view = item_view(cache, comps, item);
                    reserved_slots += view
                        .get("slots")
                        .and_then(Value::as_i64)
                        .unwrap_or(1);
                    seen_ids.insert(id.to_string());
                }
            }
            if loc_name == "CenterTorso" {
                if !has_component_category(cache, comps, &inventory, chassis, "Armor") {
                    reserved_slots += 1;
                }
                if !has_component_category(cache, comps, &inventory, chassis, "Structure") {
                    let structure_id = default_structure_id(chassis);
                    let structure_slots = component_value(cache, comps, structure_id)
                        .map(|value| component_slots(&value))
                        .unwrap_or(1);
                    reserved_slots += structure_slots;
                }
            }
            // 固定件按 Custom.Sorter 升序，且排在普通库存件之前；
            // 无 Sorter 的组件按 0 处理。
            items.sort_by(|a, b| {
                let a_fixed = (a.get("fixed").and_then(Value::as_bool).unwrap_or(false)
                    || is_linked_engine(a))
                    && !is_turret_component(a);
                let b_fixed = (b.get("fixed").and_then(Value::as_bool).unwrap_or(false)
                    || is_linked_engine(b))
                    && !is_turret_component(b);
                let a_linked = a.get("componentType").and_then(Value::as_str) == Some("Linked");
                let b_linked = b.get("componentType").and_then(Value::as_str) == Some("Linked");
                let a_linked_other = a_linked && !is_linked_engine(a);
                let b_linked_other = b_linked && !is_linked_engine(b);
                let a_cooling = is_cooling_component(a);
                let b_cooling = is_cooling_component(b);
                let a_case = !a_fixed && is_case_component(a);
                let b_case = !b_fixed && is_case_component(b);
                let a_weapon = equipment_sort_rank(a);
                let b_weapon = equipment_sort_rank(b);
                b_cooling.cmp(&a_cooling).then_with(|| {
                    a_linked_other.cmp(&b_linked_other).then_with(|| {
                        b_fixed.cmp(&a_fixed).then_with(|| {
                            a_weapon.cmp(&b_weapon).then_with(|| {
                                a_case.cmp(&b_case).then_with(|| {
                                    let a_sorter = a.get("sorter").and_then(Value::as_i64).unwrap_or(0);
                                    let b_sorter = b.get("sorter").and_then(Value::as_i64).unwrap_or(0);
                                    a_sorter.cmp(&b_sorter).then_with(|| {
                                        if a_fixed && b_fixed {
                                            let a_slots = a.get("slots").and_then(Value::as_i64).unwrap_or(1);
                                            let b_slots = b.get("slots").and_then(Value::as_i64).unwrap_or(1);
                                            b_slots.cmp(&a_slots)
                                        } else {
                                            std::cmp::Ordering::Equal
                                        }
                                    })
                                })
                            })
                        })
                    })
                })
            });
            let structure = as_i64(loc, "CurrentInternalStructure");
            let structure_max = as_i64(&chassis_loc, "InternalStructure");
            let armor = loc
                .get("AssignedArmor")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| as_i64(loc, "CurrentArmor"));
            let armor_max = as_i64(&chassis_loc, "MaxArmor");
            let rear = loc
                .get("AssignedRearArmor")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| as_i64(loc, "CurrentRearArmor"));
            let rear_max = as_i64(&chassis_loc, "MaxRearArmor");
            let slots = as_i64(&chassis_loc, "InventorySlots");
            let hardpoints = hardpoint_stats(&chassis_loc, &items);

            json!({
                "location": loc_name,
                "label": location_label(loc_name),
                "structure": { "value": structure, "max": structure_max },
                "armor": { "value": armor, "max": armor_max },
                "rearArmor": { "value": rear, "max": rear_max },
                "slots": slots,
                "reservedSlots": reserved_slots,
                "hardpoints": hardpoints,
                "items": items,
            })
        })
        .collect()
}

fn is_chassis_item(view: &Value) -> bool {
    view.get("customCategories")
        .and_then(Value::as_array)
        .map(|categories| {
            categories.iter().filter_map(Value::as_str).any(|category| {
                let category = category.to_ascii_lowercase();
                category == "armor"
                    || category == "structure"
                    || category == "superheavyframe"
                    || category == "mechtrait"
                    || category.contains("quirk")
            })
        })
        .unwrap_or(false)
}

fn chassis_items(
    cache: &DataCache,
    chassis: &Value,
    unit: &Value,
    comps: &mut HashMap<String, Value>,
) -> Vec<Value> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(fixed) = chassis.get("FixedEquipment").and_then(Value::as_array) {
        for item in fixed {
            let mut view = item_view(cache, comps, item);
            let id = view
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            // Linked_* 是装甲/结构/引擎在各部位占位的子部件，不算底盘卡片条目。
            if id.starts_with("linked_") || !is_chassis_item(&view) {
                continue;
            }
            if let Some(object) = view.as_object_mut() {
                object.insert("fixed".into(), Value::Bool(true));
            }
            let key = format!(
                "{}:{}",
                view.get("id").and_then(Value::as_str).unwrap_or(""),
                view.get("location").and_then(Value::as_str).unwrap_or("")
            );
            if seen.insert(key) {
                result.push(view);
            }
        }
    }

    if let Some(inventory) = unit.get("inventory").and_then(Value::as_array) {
        for item in inventory {
            let view = item_view(cache, comps, item);
            let id = view
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_ascii_lowercase();
            if id.starts_with("linked_") || !is_chassis_item(&view) {
                continue;
            }
            let key = format!(
                "{}:{}",
                view.get("id").and_then(Value::as_str).unwrap_or(""),
                view.get("location").and_then(Value::as_str).unwrap_or("")
            );
            if seen.insert(key) {
                result.push(view);
            }
        }
    }

    let has_armor = result.iter().any(|view| {
        view.get("customCategories")
            .and_then(Value::as_array)
            .map(|categories| {
                categories
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|category| category.eq_ignore_ascii_case("armor"))
            })
            .unwrap_or(false)
    });
    if !has_armor {
        let clan = chassis
            .pointer("/ChassisTags/items")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .any(|tag| tag.to_ascii_lowercase().contains("clan"))
            })
            .unwrap_or(false);
        let armor_id = if clan {
            "Default_Armor_Standard_Clan"
        } else {
            "Default_Armor_Standard"
        };
        if cache.component_paths.contains_key(armor_id) {
            let item = json!({
                "MountedLocation": "CenterTorso",
                "ComponentDefID": armor_id,
                "ComponentDefType": "Upgrade",
                "HardpointSlot": -1
            });
            let mut view = item_view(cache, comps, &item);
            if let Some(object) = view.as_object_mut() {
                object.insert("fixed".into(), Value::Bool(true));
                object.insert("equipment".into(), Value::Bool(true));
            }
            result.push(view);
        }
    }

    let has_structure = result.iter().any(|view| {
        view.get("customCategories")
            .and_then(Value::as_array)
            .map(|categories| {
                categories
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|category| category.eq_ignore_ascii_case("structure"))
            })
            .unwrap_or(false)
    });
    if !has_structure {
        let tags = chassis
            .pointer("/ChassisTags/items")
            .and_then(Value::as_array);
        let superheavy = tags
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .any(|tag| tag.to_ascii_lowercase().contains("superheavy"))
            })
            .unwrap_or(false);
        let lam = tags
            .map(|tags| {
                tags.iter().filter_map(Value::as_str).any(|tag| {
                    let tag = tag.to_ascii_lowercase();
                    tag.contains("landairmech") || tag == "lam"
                })
            })
            .unwrap_or(false);
        let structure_id = if lam {
            if superheavy {
                "Gear_Structure_LAM_Superheavy"
            } else {
                "Default_Structure_LAM_Standard"
            }
        } else if superheavy {
            "Default_Structure_Standard_Superheavy"
        } else {
            "Default_Structure_Standard"
        };
        if cache.component_paths.contains_key(structure_id) {
            let item = json!({
                "MountedLocation": "CenterTorso",
                "ComponentDefID": structure_id,
                "ComponentDefType": "Upgrade",
                "HardpointSlot": -1
            });
            let mut view = item_view(cache, comps, &item);
            if let Some(object) = view.as_object_mut() {
                object.insert("fixed".into(), Value::Bool(true));
            }
            result.push(view);
        }
    }

    // 游戏 Chassis 卡片顺序：Quirk -> 装甲 -> 结构 -> 其他固定件。
    result.sort_by_key(|view| {
        let categories: Vec<String> = view
            .get("customCategories")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|value| value.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        if categories.iter().any(|value| value.contains("quirk")) {
            0
        } else if categories.iter().any(|value| value == "armor") {
            1
        } else if categories.iter().any(|value| value == "structure") {
            2
        } else {
            3
        }
    });

    result
}

fn as_number(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
        .unwrap_or(0.0)
}

fn display_number(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.1}")
    }
}

fn display_integer(value: f64) -> String {
    format!("{}", value.floor() as i64)
}

fn engine_rating(
    cache: &DataCache,
    comps: &mut HashMap<String, Value>,
    inventory: &[Value],
) -> f64 {
    for item in inventory {
        let id = item
            .get("ComponentDefID")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(def) = component_value(cache, comps, id) else {
            continue;
        };
        if let Some(rating) = def.pointer("/Custom/EngineCore") {
            let value = as_number(rating);
            if value > 0.0 {
                return value;
            }
        }
    }
    0.0
}

fn engine_heat_sinking(
    cache: &DataCache,
    comps: &mut HashMap<String, Value>,
    inventory: &[Value],
) -> f64 {
    let engine_rating = engine_rating(cache, comps, inventory);
    let mut heat_sink_dissipation = 3.0;
    let mut heat_block_count = 0.0;
    let mut external_dissipation = 0.0;

    for item in inventory {
        let id = item
            .get("ComponentDefID")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(def) = component_value(cache, comps, id) else {
            continue;
        };
        // 散热套件把引擎热沉换成对应的 HeatSinkDef，直接用它的 DissipationCapacity。
        if let Some(cooling) = def
            .pointer("/Custom/Cooling/HeatSinkDefId")
            .and_then(Value::as_str)
        {
            if let Some(hs_def) = component_value(cache, comps, cooling) {
                let dissipation = as_f64(&hs_def, "DissipationCapacity").max(0.0);
                if dissipation > 0.0 {
                    heat_sink_dissipation = dissipation;
                }
            }
        }
        if let Some(block) = def.pointer("/Custom/EngineHeatBlock") {
            heat_block_count += as_number(block).max(0.0);
        }
        // 外部热沉：按 Heatsink 分类，每个热沉只计一次 DissipationCapacity。
        let is_heatsink = def
            .pointer("/Custom/Category")
            .and_then(Value::as_array)
            .map(|categories| {
                categories.iter().any(|category| {
                    category
                        .get("CategoryID")
                        .and_then(Value::as_str)
                        .map(|value| value.eq_ignore_ascii_case("heatsink"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if is_heatsink {
            external_dissipation += as_f64(&def, "DissipationCapacity").max(0.0);
        }
    }

    // MechEngineer 在没有 EngineCore 时使用 EngineMissingFallbackHeatSinkCapacity=30。
    if engine_rating <= 0.0 {
        return 30.0;
    }

    let free_sinks = (engine_rating / 25.0).floor().clamp(0.0, 10.0);
    (free_sinks + heat_block_count) * heat_sink_dissipation + external_dissipation
}

fn end_move_heat(
    cache: &DataCache,
    comps: &mut HashMap<String, Value>,
    inventory: &[Value],
) -> f64 {
    let mut total = 0.0;
    for item in inventory {
        let id = item
            .get("ComponentDefID")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(def) = component_value(cache, comps, id) else {
            continue;
        };
        for key in ["/statusEffects", "/Custom/statusEffects"] {
            let Some(effects) = def.pointer(key).and_then(Value::as_array) else {
                continue;
            };
            for effect in effects {
                let stat_name = effect
                    .pointer("/statisticData/statName")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !stat_name.eq_ignore_ascii_case("EndMoveHeat") {
                    continue;
                }
                let value = effect
                    .pointer("/statisticData/modValue")
                    .map(as_number)
                    .unwrap_or(0.0);
                total += value;
            }
        }
    }
    total
}

fn heat_modifiers(
    cache: &DataCache,
    comps: &mut HashMap<String, Value>,
    inventory: &[Value],
) -> (f64, f64) {
    let mut heat_generated = 1.0;
    let mut weapon_heat = 1.0;

    let apply = |current: f64, operation: &str, value: f64| -> f64 {
        match operation {
            "Float_Multiply" => current * value,
            "Float_Add" | "Int_Add" => current + value,
            "Float_Subtract" | "Int_Subtract" => current - value,
            "Float_Divide" => current / value,
            _ => current,
        }
    };

    for item in inventory {
        let id = item
            .get("ComponentDefID")
            .and_then(Value::as_str)
            .unwrap_or("");
        let Some(def) = component_value(cache, comps, id) else {
            continue;
        };
        for effect in def.get("statusEffects").and_then(Value::as_array).into_iter().flatten() {
            if effect
                .pointer("/targetingData/effectTriggerType")
                .and_then(Value::as_str)
                != Some("Passive")
            {
                continue;
            }
            if effect
                .pointer("/targetingData/effectTargetType")
                .and_then(Value::as_str)
                != Some("Creator")
            {
                continue;
            }
            if effect
                .pointer("/statisticData/targetCollection")
                .and_then(Value::as_str)
                != Some("Weapon")
            {
                continue;
            }
            let stat = effect
                .pointer("/statisticData/statName")
                .and_then(Value::as_str)
                .unwrap_or("");
            let operation = effect
                .pointer("/statisticData/operation")
                .and_then(Value::as_str)
                .unwrap_or("");
            let value = effect
                .pointer("/statisticData/modValue")
                .map(as_number)
                .unwrap_or(0.0);
            if stat == "HeatGenerated" {
                heat_generated = apply(heat_generated, operation, value);
            } else if stat == "WeaponHeatMultiplier" {
                weapon_heat = apply(weapon_heat, operation, value);
            }
        }
    }

    (heat_generated, weapon_heat)
}

fn mech_ratings(
    cache: &DataCache,
    unit: &Value,
    chassis: &Value,
    comps: &mut HashMap<String, Value>,
) -> Vec<Value> {
    let mut inventory = unit
        .get("inventory")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // 引擎、陀螺仪等固定件在 chassis.FixedEquipment 中，评级计算也需要纳入。
    if let Some(fixed) = chassis.get("FixedEquipment").and_then(Value::as_array) {
        inventory.extend(fixed.iter().cloned());
    }
    let mut near = 0.0;
    let mut mid = 0.0;
    let mut long = 0.0;
    let mut very_long = 0.0;
    let mut indirect = 0.0;
    let (heat_generated_multiplier, weapon_heat_multiplier) =
        heat_modifiers(cache, comps, &inventory);
    let mut alpha_heat = 0.0;
    let mut all_damage = 0.0;
    let mut all_instability = 0.0;
    let mut all_heat_damage = 0.0;
    let mut all_structure_damage = 0.0;
    let mut near_instability = 0.0;
    let mut near_heat_damage = 0.0;
    let mut near_structure_damage = 0.0;
    let mut mid_instability = 0.0;
    let mut mid_heat_damage = 0.0;
    let mut mid_structure_damage = 0.0;
    let mut mid_accuracy = 0.0;
    let mut mid_count = 0.0;
    let mut long_instability = 0.0;
    let mut long_heat_damage = 0.0;
    let mut long_structure_damage = 0.0;
    let mut long_accuracy = 0.0;
    let mut long_count = 0.0;

    for item in &inventory {
        let ctype = item
            .get("ComponentDefType")
            .and_then(Value::as_str)
            .unwrap_or("");
        let id = item
            .get("ComponentDefID")
            .and_then(Value::as_str)
            .unwrap_or("");
        if ctype == "HeatSink" {
            // 引擎热沉在循环结束后按 EngineHeatDissipation 单独计算。
            continue;
        }
        if ctype != "Weapon" {
            continue;
        }
        let Some(def) = component_value(cache, comps, id) else {
            continue;
        };
        let shots = as_f64(&def, "ShotsWhenFired").max(1.0);
        let damage = as_f64(&def, "Damage") * shots;
        let instability = as_f64(&def, "Instability") * shots;
        let heat_damage = as_f64(&def, "HeatDamage") * shots;
        let structure_damage = {
            let value = as_f64(&def, "StructureDamage");
            if value > 0.0 {
                value * shots
            } else {
                as_f64(&def, "APDamage") * shots
            }
        };
        let accuracy = as_f64(&def, "AccuracyModifier");
        let heat = (as_f64(&def, "HeatGenerated")
            * heat_generated_multiplier
            * weapon_heat_multiplier) as i64 as f64;
        let min_range = as_f64(&def, "MinRange");
        let max_range = as_f64(&def, "MaxRange");
        alpha_heat += heat;
        all_damage += damage;
        all_instability += instability;
        all_heat_damage += heat_damage;
        all_structure_damage += structure_damage;
        // 与 MechEngineer 的 Close/Medium/Long Range 统计一致：
        // 按武器射程与区间是否有重叠来统计，不只看 MaxRange。
        if min_range <= 89.0 && max_range >= 0.0 {
            near += damage;
            near_instability += instability;
            near_heat_damage += heat_damage;
            near_structure_damage += structure_damage;
        }
        if min_range <= 359.0 && max_range >= 90.0 {
            mid += damage;
            mid_instability += instability;
            mid_heat_damage += heat_damage;
            mid_structure_damage += structure_damage;
            mid_accuracy += accuracy;
            mid_count += 1.0;
        }
        if max_range >= 360.0 {
            long += damage;
            long_instability += instability;
            long_heat_damage += heat_damage;
            long_structure_damage += structure_damage;
            long_accuracy += accuracy;
            long_count += 1.0;
        }
        if max_range >= 570.0 {
            very_long += damage;
        }
        if def
            .get("IndirectFireCapable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            indirect += damage;
        }
    }

    // Close Range 评分会额外加入机体近战伤害。
    near += as_f64(chassis, "MeleeDamage");

    let dissipation = engine_heat_sinking(cache, comps, &inventory);
    let end_move_heat = end_move_heat(cache, comps, &inventory);
    let armor: f64 = unit
        .get("Locations")
        .and_then(Value::as_array)
        .map(|locs| {
            locs.iter()
                .map(|l| {
                    l.get("AssignedArmor")
                        .and_then(Value::as_f64)
                        .unwrap_or_else(|| as_f64(l, "CurrentArmor"))
                        + l.get("AssignedRearArmor")
                            .and_then(Value::as_f64)
                            .unwrap_or_else(|| as_f64(l, "CurrentRearArmor"))
                })
                .sum()
        })
        .unwrap_or(0.0);
    // 步行 MP 按引擎等级 / 吨位计算（超重型机体 TopSpeed 不代表实际 MP）。
    let engine_rating = engine_rating(cache, comps, &inventory);
    let tonnage = as_f64(chassis, "Tonnage").max(1.0);
    let walk_mp = if engine_rating > 0.0 {
        (engine_rating / tonnage).floor()
    } else {
        (as_f64(chassis, "TopSpeed") / 16.0).floor()
    };
    let walk_speed = walk_mp * 25.0;
    let run_mp = (walk_mp * 1.5).ceil();
    let run_speed = run_mp * 25.0;
    let structure: f64 = unit
        .get("Locations")
        .and_then(Value::as_array)
        .map(|locations| {
            locations
                .iter()
                .map(|location| as_f64(location, "CurrentInternalStructure"))
                .sum()
        })
        .unwrap_or(0.0);
    let stability = as_f64(chassis, "Stability");
    let unsteady = (stability * 0.41).round();
    let melee_damage = as_f64(chassis, "MeleeDamage");
    let melee_instability = as_f64(chassis, "MeleeInstability");
    let jump_heat = 9.0;
    let close_melee_damage = all_damage + melee_damage;
    let close_melee_stability = all_instability + melee_instability;

    let firepower_bars = |raw: f64, min: f64, max: f64| -> i64 {
        let fraction = ((raw - min) / (max - min)).clamp(0.0, 1.0);
        ((fraction * 9.0 + 1.0).floor() as i64).clamp(1, 10)
    };

    vec![
        json!({
            "key":"near",
            "label":"近程",
            "score": near.round() as i64,
            "value": firepower_bars(near, 65.0, 495.0),
            "details": [
                {"label":"近战伤害","value": display_number(close_melee_damage)},
                {"label":"远程伤害","value":"0"},
                {"label":"近战稳定伤害","value": display_integer(close_melee_stability)},
                {"label":"远程稳定伤害","value":"0"},
                {"label":"近战.热量/结构伤害","value": format!("{}/{}", display_integer(near_heat_damage), display_integer(near_structure_damage))},
                {"label":"远程.热量/结构伤害","value":"0/0"},
            ],
            "description":"包含 90 米内可开火的武器、近战与支援武器，以及 DFA 的相关数据。"
        }),
        json!({
            "key":"heat",
            "label":"热效率",
            "value": if alpha_heat < 1.0 {
                1
            } else {
                firepower_bars(dissipation - end_move_heat, alpha_heat / 3.0, alpha_heat)
            },
            "details": [
                {"label":"散热","value": format!("{} Heat", display_number(-dissipation + end_move_heat))},
                {"label":"齐射热量","value": format!("{} Heat", display_number(alpha_heat))},
                {"label":"回合结束","value": format!("{} Heat", display_number(-dissipation))},
                {"label":"移动结束","value": format!("{} Heat", display_number(end_move_heat))},
                {"label":"热量差","value": format!("{} Heat", display_number(alpha_heat - dissipation + end_move_heat))},
                {"label":"跳跃","value": format!("{} Heat", display_number(jump_heat))},
            ],
            "description":"热效率由机甲的总散热能力，与每轮武器产生的总热量对比决定。"
        }),
        json!({
            "key":"mid",
            "label":"中程",
            "score": mid.round() as i64,
            "value": firepower_bars(mid, 50.0, 355.0),
            "details": [
                {"label":"中程伤害","value": display_number(mid)},
                {"label":"结构伤害","value": display_integer(mid_structure_damage)},
                {"label":"失衡伤害","value": display_integer(mid_instability)},
                {"label":"平均命中","value": if mid_count > 0.0 { format!("-{:.0}", mid_accuracy / mid_count) } else { "-0".into() }},
                {"label":"热量伤害","value": display_integer(mid_heat_damage)},
            ],
            "description":"中距离火力，由所有射程覆盖中程区间的武器可能造成的伤害合计决定。"
        }),
        json!({
            "key":"move",
            "label":"移动力",
            "score": run_speed.round() as i64,
            "value": firepower_bars(run_speed, 36.0, 350.0),
            "details": [
                {"label":"最大移动","value": format!("{}m / {} hex", display_number(walk_speed), (walk_speed / 25.0).floor() as i64)},
                {"label":"最大冲刺","value": format!("{}m / {} hex", display_number(run_speed), (run_speed / 25.0).floor() as i64)},
                {"label":"最大跳跃","value":"0m / 0 hex"},
                {"label":"TT 步行 MP","value": display_number(walk_mp)},
            ],
            "description":"移动能力由机甲的移动速度合计决定。"
        }),
        json!({
            "key":"long",
            "label":"长程",
            "score": long.round() as i64,
            "value": firepower_bars(long, 50.0, 355.0),
            "details": [
                {"label":"长程伤害","value": display_number(long)},
                {"label":"稳定伤害","value": display_integer(long_instability)},
                {"label":"超长程伤害","value": display_number(very_long)},
                {"label":"热量/结构伤害","value": format!("{}/{}", display_integer(long_heat_damage), display_integer(long_structure_damage))},
                {"label":"间接伤害","value": display_number(indirect)},
                {"label":"平均命中","value": if long_count > 0.0 { format!("-{:.0}", long_accuracy / long_count) } else { "-0".into() }},
            ],
            "description":"机甲在远距离上的相对火力。"
        }),
        json!({
            "key":"durability",
            "label":"耐久度",
            "score": armor.round() as i64,
            "value": firepower_bars(armor, 0.0, 1600.0),
            "details": [
                {"label":"装甲","value": display_number(armor)},
                {"label":"结构","value": display_number(structure)},
                {"label":"伤害减免","value":"0%"},
                {"label":"稳定/失衡阈值","value": format!("{} / {}", display_number(stability), display_number(unsteady))},
                {"label":"最大闪避","value":"8"},
                {"label":"防御","value":"0"},
            ],
            "description":"耐久度由机甲所有部位的装甲合计决定。"
        }),
    ]
}

fn assembly_names(chassis: &Value) -> Vec<String> {
    let mut names = Vec::new();
    for key in ["PrefabBase", "PrefabIdentifier"] {
        let value = chassis.get(key).and_then(Value::as_str).unwrap_or("");
        if !value.is_empty() {
            names.push(value.to_string());
        }
    }
    for path in [
        &["Custom", "AssemblyVariant", "PrefabID"][..],
        &["Custom", "VAssemblyVariant", "PrefabID"][..],
    ] {
        let value = pointer_str(chassis, path);
        if !value.is_empty() {
            names.push(value.to_string());
        }
    }
    let tonnage = as_f64(chassis, "Tonnage");
    if tonnage > 0.0 {
        let base: Vec<String> = names.clone();
        for name in base {
            names.push(format!("{}_{}", name, tonnage.round() as i64));
        }
    }
    names
}

fn arm_limits(chassis: &Value) -> Value {
    let tags: Vec<String> = chassis
        .pointer("/ChassisTags/items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    let has = |tag: &str| tags.iter().any(|item| item == tag);

    let left_upper = has("armlimitupperleft");
    let right_upper = has("armlimitupperright");
    let left_lower_limit = has("armlimitlowerleft");
    let right_lower_limit = has("armlimitlowerright");

    json!({
        "LeftArm": {
            "lower": !left_upper,
            "hand": !left_upper && !left_lower_limit,
        },
        "RightArm": {
            "lower": !right_upper,
            "hand": !right_upper && !right_lower_limit,
        },
    })
}

fn affinity_matches(chassis: &Value, unit: &Value, affinity: &Value) -> bool {
    let Some(data) = affinity.get("affinityData") else {
        return false;
    };

    // Quirk 类亲和按机体实际安装的装备/特质匹配，例如 Command Mech -> High Command。
    if let Some(quirk_names) = data.get("quirkNames").and_then(Value::as_array) {
        let mut installed = std::collections::HashSet::new();
        if let Some(items) = unit.get("inventory").and_then(Value::as_array) {
            for item in items {
                if let Some(id) = item.get("ComponentDefID").and_then(Value::as_str) {
                    installed.insert(id.to_ascii_lowercase());
                }
            }
        }
        if let Some(items) = chassis.get("FixedEquipment").and_then(Value::as_array) {
            for item in items {
                if let Some(id) = item.get("ComponentDefID").and_then(Value::as_str) {
                    installed.insert(id.to_ascii_lowercase());
                }
            }
        }
        if quirk_names.iter().filter_map(Value::as_str).any(|id| {
            installed.contains(&id.to_ascii_lowercase())
        }) {
            return true;
        }
    }

    let Some(names) = data.get("chassisNames").and_then(Value::as_array) else {
        return false;
    };
    let names: Vec<String> = names
        .iter()
        .filter_map(Value::as_str)
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let id_type = data
        .get("idType")
        .and_then(Value::as_str)
        .unwrap_or("AssemblyVariant");
    let chassis_id = def_id(chassis).to_ascii_lowercase();
    if id_type.eq_ignore_ascii_case("ChassisId") && names.contains(&chassis_id) {
        return true;
    }
    let candidates = assembly_names(chassis)
        .into_iter()
        .map(|s| s.to_ascii_lowercase())
        .collect::<Vec<_>>();
    candidates.iter().any(|candidate| names.contains(candidate))
}

fn affinity_levels(cache: &DataCache, chassis: &Value, unit: &Value) -> Vec<Value> {
    let mut result = Vec::new();
    for affinity in &cache.affinities {
        if !affinity_matches(chassis, unit, affinity) {
            continue;
        }
        let Some(data) = affinity.get("affinityData") else {
            continue;
        };
        let Some(levels) = data.get("affinityLevels").and_then(Value::as_array) else {
            continue;
        };
        for level in levels {
            result.push(json!({
                "name": level.get("levelName").and_then(Value::as_str).unwrap_or(""),
                "missions": level.get("missionsRequired").and_then(Value::as_i64).unwrap_or(0),
                "description": level.get("decription").and_then(Value::as_str)
                    .or_else(|| level.get("description").and_then(Value::as_str))
                    .unwrap_or(""),
            }));
        }
    }
    let mut seen = std::collections::HashSet::new();
    result.retain(|item| {
        let key = format!(
            "{}:{}:{}",
            item.get("missions").and_then(Value::as_i64).unwrap_or(0),
            item.get("name").and_then(Value::as_str).unwrap_or(""),
            item.get("description").and_then(Value::as_str).unwrap_or("")
        );
        seen.insert(key)
    });
    result.sort_by_key(|item| item.get("missions").and_then(Value::as_i64).unwrap_or(0));
    result
}

fn installed_component_categories(
    cache: &DataCache,
    comps: &mut HashMap<String, Value>,
    unit: &Value,
    chassis: &Value,
) -> std::collections::HashSet<String> {
    let mut result = std::collections::HashSet::new();
    let mut scan = |items: &[Value]| {
        for item in items {
            let id = item
                .get("ComponentDefID")
                .and_then(Value::as_str)
                .unwrap_or("");
            let Some(def) = component_value(cache, comps, id) else {
                continue;
            };
            if let Some(categories) = def
                .pointer("/Custom/Category")
                .and_then(Value::as_array)
            {
                for category in categories {
                    if let Some(category_id) =
                        category.get("CategoryID").and_then(Value::as_str)
                    {
                        result.insert(category_id.to_ascii_lowercase());
                    }
                }
            }
        }
    };
    if let Some(inventory) = unit.get("inventory").and_then(Value::as_array) {
        scan(inventory);
    }
    if let Some(fixed) = chassis.get("FixedEquipment").and_then(Value::as_array) {
        scan(fixed);
    }
    result
}

fn mech_default_items(cache: &DataCache, chassis: &Value) -> Vec<Value> {
    let Some(defaults) = cache.mech_defaults.as_ref() else {
        return Vec::new();
    };
    let Some(settings) = defaults.get("Settings").and_then(Value::as_array) else {
        return Vec::new();
    };
    let tags: std::collections::HashSet<String> = chassis
        .pointer("/ChassisTags/items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();

    let mut result = Vec::new();
    for setting in settings {
        let category = setting
            .get("CategoryID")
            .and_then(Value::as_str)
            .unwrap_or("");
        // 这些是安装位置固定的冷却/引擎默认件；其余默认件由机体 inventory 或前端展示。
        if category != "Cooling" && category != "EngineHeatBlock" {
            continue;
        }
        let mut chosen = setting.get("Defaults").and_then(Value::as_array);
        if let Some(unit_types) = setting.get("UnitTypes").and_then(Value::as_array) {
            for unit_type in unit_types {
                let name = unit_type
                    .get("UnitType")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if !name.is_empty() && tags.contains(&name.to_ascii_lowercase()) {
                    chosen = unit_type.get("Defaults").and_then(Value::as_array);
                    break;
                }
            }
        }
        for item in chosen.into_iter().flatten() {
            let location = item.get("Location").and_then(Value::as_str).unwrap_or("");
            let def_id = item.get("DefID").and_then(Value::as_str).unwrap_or("");
            let component_type = item
                .get("Type")
                .and_then(Value::as_str)
                .unwrap_or("HeatSink");
            if location.is_empty() || def_id.is_empty() {
                continue;
            }
            result.push(json!({
                "MountedLocation": location,
                "ComponentDefID": def_id,
                "ComponentDefType": component_type,
                "HardpointSlot": -1,
                "IsFixed": true,
            }));
        }
    }
    result
}

fn is_battle_armor(chassis: &Value) -> bool {
    let has_tag = chassis
        .pointer("/ChassisTags/items")
        .and_then(Value::as_array)
        .map(|tags| {
            tags.iter().filter_map(Value::as_str).any(|tag| {
                tag.eq_ignore_ascii_case("BattleArmor")
                    || tag.eq_ignore_ascii_case("Battle Armor")
            })
        })
        .unwrap_or(false);
    has_tag
        || chassis
            .pointer("/CustomParts/SquadInfo/Troopers")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            > 0
}

fn battle_armor_item_view(
    cache: &DataCache,
    comps: &mut HashMap<String, Value>,
    item: &Value,
) -> Value {
    let mut view = item_view(cache, comps, item);
    if let Some(object) = view.as_object_mut() {
        object.insert("fixed".into(), Value::Bool(true));
    }
    view
}

fn battle_armor_section(
    location: &str,
    label: &str,
    slots: i64,
    items: Vec<Value>,
    armor_value: f64,
    armor_max: f64,
    structure_value: f64,
    structure_max: f64,
) -> Value {
    json!({
        "location": location,
        "label": label,
        "slots": slots.max(1),
        "reservedSlots": 0,
        "hardpoints": [],
        "items": items,
        "armor": { "value": armor_value, "max": armor_max },
        "rearArmor": { "value": 0, "max": 0 },
        "structure": { "value": structure_value, "max": structure_max },
    })
}

/// 动力装甲在游戏里不是普通机甲的部位布局：
/// Chassis / Trooper 1-4 / Internals。
/// 固定件按组件 ID 分类，而不是按 Head/CT 等原始位置显示。
fn battle_armor_sections(
    cache: &DataCache,
    chassis: &Value,
    unit: &Value,
    comps: &mut HashMap<String, Value>,
) -> (Vec<Value>, Vec<Value>) {
    let mut chassis_items = Vec::new();
    let mut internals = Vec::new();
    let mut trooper_fixed: HashMap<String, Vec<Value>> = HashMap::new();

    if let Some(fixed) = chassis.get("FixedEquipment").and_then(Value::as_array) {
        for item in fixed {
            let id = item
                .get("ComponentDefID")
                .and_then(Value::as_str)
                .unwrap_or("");
            if id.starts_with("Gear_Trooper_Weight") {
                continue;
            }
            let view = battle_armor_item_view(cache, comps, item);
            let lower = id.to_ascii_lowercase();
            if lower.starts_with("quirk_battlearmor")
                || lower.starts_with("gear_battlearmor_armor")
                || lower == "gear_battlearmor_structure"
            {
                chassis_items.push(view);
            } else if lower.starts_with("gear_battlearmor_controls")
                || lower == "gear_battlearmor_systems"
                || lower.starts_with("gear_battlearmor_enginecore")
                || lower.starts_with("gear_battlearmor_motivesystem")
            {
                internals.push(view);
            } else {
                let location = item
                    .get("MountedLocation")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                trooper_fixed.entry(location).or_default().push(view);
            }
        }
    }

    let mut trooper_locations = Vec::new();
    if let Some(locations) = chassis.get("Locations").and_then(Value::as_array) {
        for location in locations {
            let has_ba_hardpoint = location
                .get("Hardpoints")
                .and_then(Value::as_array)
                .map(|hardpoints| {
                    hardpoints.iter().any(|hardpoint| {
                        hardpoint
                            .get("WeaponMountID")
                            .and_then(Value::as_str)
                            .map(|mount| mount.eq_ignore_ascii_case("BattleArmor"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if has_ba_hardpoint && as_i64(location, "InventorySlots") > 0 {
                trooper_locations.push(
                    location
                        .get("Location")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                );
            }
        }
    }
    if trooper_locations.is_empty() {
        trooper_locations = vec![
            "Head".to_string(),
            "LeftTorso".to_string(),
            "CenterTorso".to_string(),
            "RightTorso".to_string(),
        ];
    }

    let troopers = chassis
        .pointer("/CustomParts/SquadInfo/Troopers")
        .and_then(Value::as_i64)
        .unwrap_or(trooper_locations.len() as i64)
        .clamp(1, trooper_locations.len() as i64) as usize;

    let mut sections = Vec::new();
    for (index, location) in trooper_locations.iter().take(troopers).enumerate() {
        let mut items: Vec<Value> = trooper_fixed.remove(location).unwrap_or_default();
        items.sort_by_key(|item| {
            let id = item.get("id").and_then(Value::as_str).unwrap_or("");
            match id {
                "Gear_BattleArmor_BasicManipulators" => 0,
                "Gear_BattleArmor_Harjel" => 0,
                "Gear_BattleArmor_JumpJet_3" => 1,
                _ => 2,
            }
        });
        if let Some(inventory) = unit.get("inventory").and_then(Value::as_array) {
            for item in inventory {
                if item.get("MountedLocation").and_then(Value::as_str) != Some(location.as_str()) {
                    continue;
                }
                // 同 ID 的多件武器也可能是独立槽位（例如双联 ER Micro Laser），
                // 动力装甲这里不能按 ComponentDefID 去重。
                items.push(item_view(cache, comps, item));
            }
        }
        let slots = items
            .iter()
            .map(|item| {
                item.get("slots")
                    .and_then(Value::as_i64)
                    .unwrap_or(1)
                    .max(1)
            })
            .sum::<i64>()
            .max(items.len() as i64);
        let chassis_location = chassis
            .get("Locations")
            .and_then(Value::as_array)
            .and_then(|locations| {
                locations.iter().find(|candidate| {
                    candidate.get("Location").and_then(Value::as_str) == Some(location.as_str())
                })
            });
        let armor_max = chassis_location
            .map(|location| as_f64(location, "MaxArmor"))
            .unwrap_or(0.0);
        let structure_max = chassis_location
            .map(|location| as_f64(location, "InternalStructure"))
            .unwrap_or(0.0);
        let unit_location = unit
            .get("Locations")
            .and_then(Value::as_array)
            .and_then(|locations| {
                locations.iter().find(|candidate| {
                    candidate.get("Location").and_then(Value::as_str) == Some(location.as_str())
                })
            });
        let armor_value = unit_location
            .and_then(|location| {
                location
                    .get("CurrentArmor")
                    .and_then(Value::as_f64)
                    .or_else(|| location.get("AssignedArmor").and_then(Value::as_f64))
            })
            .unwrap_or(armor_max);
        let structure_value = unit_location
            .and_then(|location| {
                location
                    .get("CurrentInternalStructure")
                    .and_then(Value::as_f64)
            })
            .unwrap_or(structure_max);
        sections.push(battle_armor_section(
            &format!("Trooper{}", index + 1),
            &format!("Trooper {}", index + 1),
            slots,
            items,
            armor_value,
            armor_max,
            structure_value,
            structure_max,
        ));
    }

    internals.sort_by_key(|item| {
        let id = item.get("id").and_then(Value::as_str).unwrap_or("");
        if id == "Gear_BattleArmor_Systems" {
            0
        } else if id.starts_with("Gear_BattleArmor_Controls") {
            1
        } else if id.starts_with("Gear_BattleArmor_EngineCore") {
            2
        } else if id.starts_with("Gear_BattleArmor_MotiveSystem") {
            3
        } else {
            4
        }
    });
    let internals_slots = internals
        .iter()
        .map(|item| {
            item.get("slots")
                .and_then(Value::as_i64)
                .unwrap_or(1)
                .max(1)
        })
        .sum::<i64>()
        .max(internals.len() as i64);
    sections.push(battle_armor_section(
        "Internals",
        "Internals",
        internals_slots,
        internals,
        0.0,
        0.0,
        0.0,
        0.0,
    ));

    (sections, chassis_items)
}

fn battle_armor_jumpjet_max(
    cache: &DataCache,
    comps: &mut HashMap<String, Value>,
    chassis: &Value,
) -> Option<i64> {
    let troopers = chassis
        .pointer("/CustomParts/SquadInfo/Troopers")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let mut jump_systems = 0;
    if let Some(fixed) = chassis.get("FixedEquipment").and_then(Value::as_array) {
        for item in fixed {
            if item.get("ComponentDefType").and_then(Value::as_str) != Some("JumpJet") {
                continue;
            }
            let id = item
                .get("ComponentDefID")
                .and_then(Value::as_str)
                .unwrap_or("");
            let Some(def) = component_value(cache, comps, id) else {
                continue;
            };
            if let Some(bonuses) = def
                .pointer("/Custom/BonusDescriptions")
                .and_then(Value::as_array)
            {
                for bonus in bonuses.iter().filter_map(Value::as_str) {
                    if let Some(value) = bonus.strip_prefix("BAJumpSystems:") {
                        jump_systems = jump_systems.max(
                            value.trim().parse::<i64>().unwrap_or(0),
                        );
                    }
                }
            }
        }
    }
    if troopers > 0 && jump_systems > 0 {
        Some(troopers + jump_systems)
    } else {
        None
    }
}

fn mech_detail(cache: &DataCache, entry: &DataEntry) -> Result<Value, String> {
    let mech = read_json(Path::new(&entry.path))
        .ok_or_else(|| format!("读取机甲文件失败: {}", entry.path))?;
    let chassis_id = mech
        .get("ChassisID")
        .and_then(Value::as_str)
        .unwrap_or("");
    let chassis_path = cache
        .chassis_paths
        .get(chassis_id)
        .ok_or_else(|| format!("找不到底盘文件: {chassis_id}"))?;
    let chassis = read_json(chassis_path)
        .ok_or_else(|| format!("读取底盘文件失败: {}", chassis_path.display()))?;

    let mut layout_unit = mech.clone();
    let battle_armor = is_battle_armor(&chassis);
    let mut comps = HashMap::new();
    if !battle_armor {
        let defaults = mech_default_items(cache, &chassis);
        let installed =
            installed_component_categories(cache, &mut comps, &layout_unit, &chassis);
        if let Some(inventory) = layout_unit.get_mut("inventory").and_then(Value::as_array_mut) {
            for item in defaults {
                let id = item
                    .get("ComponentDefID")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let exists = inventory.iter().any(|existing| {
                    existing.get("ComponentDefID").and_then(Value::as_str) == Some(id)
                });
                if exists {
                    continue;
                }
                let default_categories = component_value(cache, &mut comps, id)
                    .and_then(|definition| {
                        definition
                            .pointer("/Custom/Category")
                            .and_then(Value::as_array)
                            .map(|categories| {
                                categories
                                    .iter()
                                    .filter_map(|category| {
                                        category.get("CategoryID").and_then(Value::as_str)
                                    })
                                    .map(|category| category.to_ascii_lowercase())
                                    .collect::<Vec<_>>()
                            })
                    })
                    .unwrap_or_default();
                // EnginePart 是引擎组件的通用分类，不能代表 Cooling /
                // EngineHeatBlock 已经安装，否则会导致默认件被误跳过。
                if default_categories
                    .iter()
                    .any(|category| category != "enginepart" && installed.contains(category))
                {
                    continue;
                }
                inventory.push(item);
            }
        }
    }
    let (locations, chassis_slots) = if battle_armor {
        battle_armor_sections(cache, &chassis, &layout_unit, &mut comps)
    } else {
        (
            build_locations(cache, &layout_unit, &chassis, &mut comps),
            chassis_items(cache, &chassis, &mech, &mut comps),
        )
    };
    let ratings = mech_ratings(cache, &layout_unit, &chassis, &mut comps);
    let affinities = affinity_levels(cache, &chassis, &layout_unit);
    let handheld_slot = component_value(cache, &mut comps, "Default_HandHeld_Slot");
    let handheld_slot_name = handheld_slot
        .as_ref()
        .map(|value| display_name(value, "HandHeld Slot"))
        .unwrap_or_else(|| "HandHeld Slot".to_string());
    let handheld_slot_icon = handheld_slot.as_ref().and_then(|value| {
        let icon = pointer_str(value, &["Description", "Icon"]);
        cache
            .icon_paths
            .get(&icon.to_ascii_lowercase())
            .map(|path| path.to_string_lossy().to_string())
    });
    let special_slot = component_value(cache, &mut comps, "Default_AddOn_Slot");
    let special_slot_name = special_slot
        .as_ref()
        .map(|value| display_name(value, "Specialist Slot"))
        .unwrap_or_else(|| "Specialist Slot".to_string());
    let special_slot_icon = special_slot.as_ref().and_then(|value| {
        let icon = pointer_str(value, &["Description", "Icon"]);
        cache
            .icon_paths
            .get(&icon.to_ascii_lowercase())
            .map(|path| path.to_string_lossy().to_string())
    });
    let handheld_slot_color = handheld_slot
        .as_ref()
        .map(|value| resolve_color_tag(cache, value))
        .unwrap_or_default();
    let special_slot_color = special_slot
        .as_ref()
        .map(|value| resolve_color_tag(cache, value))
        .unwrap_or_default();
    // 空的手持/特殊槽位只显示分类颜色，不叠加固定件预装底色。
    let handheld_slot_preinstalled = false;
    let special_slot_preinstalled = false;

    let actuator_icons = json!({
        "Head": {
            "cockpit": actuator_icon_entry(cache, &mut comps, "Default_Cockpit_Standard"),
            "lifesupporta": actuator_icon_entry(cache, &mut comps, "Default_Cockpit_LifeSupportA_Torso"),
            "lifesupportb": actuator_icon_entry(cache, &mut comps, "Default_Cockpit_LifeSupportB_Torso"),
            "sensors": actuator_icon_entry(cache, &mut comps, "Default_Sensors_Standard"),
            "fcs": actuator_icon_entry(cache, &mut comps, "Default_FCS_Standard"),
        },
        "Arm": {
            "shoulder": actuator_icon_entry(cache, &mut comps, "Default_Actuator_Arm_Shoulder"),
            "upper": actuator_icon_entry(cache, &mut comps, "Default_Actuator_Arm_Upper"),
            "lower": actuator_icon_entry(cache, &mut comps, "Default_Actuator_Arm_Lower"),
            "hand": actuator_icon_entry(cache, &mut comps, "Default_Actuator_Arm_Hand"),
        },
        "Leg": {
            "hip": actuator_icon_entry(cache, &mut comps, "Default_Actuator_Leg_Hip"),
            "upper": actuator_icon_entry(cache, &mut comps, "Default_Actuator_Leg_Upper"),
            "lower": actuator_icon_entry(cache, &mut comps, "Default_Actuator_Leg_Lower"),
            "foot": actuator_icon_entry(cache, &mut comps, "Default_Actuator_Leg_Foot"),
        },
    });

    let name = display_name(&mech, &entry.id);
    let variant = chassis
        .get("VariantName")
        .and_then(Value::as_str)
        .unwrap_or("");
    let walk_mp = (as_i64(&chassis, "TopSpeed") / 16).max(0);
    let chassis_max_jumpjets = as_i64(&chassis, "MaxJumpjets");
    let max_jumpjets = if battle_armor {
        battle_armor_jumpjet_max(cache, &mut comps, &chassis).unwrap_or_else(|| {
            if chassis_max_jumpjets > 0 && walk_mp > 0 {
                chassis_max_jumpjets.min(walk_mp)
            } else {
                chassis_max_jumpjets
            }
        })
    } else if chassis_max_jumpjets > 0 && walk_mp > 0 {
        chassis_max_jumpjets.min(walk_mp)
    } else {
        chassis_max_jumpjets
    };
    let inventory_jumpjets = mech
        .get("inventory")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("ComponentDefType").and_then(Value::as_str) == Some("JumpJet")
                })
                .count() as i64
        })
        .unwrap_or(0);
    let fixed_jumpjets = chassis
        .get("FixedEquipment")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("ComponentDefType").and_then(Value::as_str) == Some("JumpJet")
                })
                .count() as i64
        })
        .unwrap_or(0);
    let jumpjets = inventory_jumpjets + fixed_jumpjets;

    let mut max_hp: HashMap<&'static str, i64> = HashMap::new();
    let mut used_hp: HashMap<&'static str, i64> = HashMap::new();
    let mut omni_locations = std::collections::HashSet::new();
    if let Some(locs) = chassis.get("Locations").and_then(Value::as_array) {
        for loc in locs {
            if let Some(hps) = loc.get("Hardpoints").and_then(Value::as_array) {
                for hp in hps {
                    let omni = hp.get("Omni").and_then(Value::as_bool).unwrap_or(false);
                    if omni {
                        if let Some(location) = loc.get("Location").and_then(Value::as_str) {
                            omni_locations.insert(location.to_string());
                        }
                    }
                    let key = if omni {
                        "universal"
                    } else {
                        hardpoint_key(
                            hp.get("WeaponMountID")
                                .and_then(Value::as_str)
                                .unwrap_or(""),
                        )
                    };
                    *max_hp.entry(key).or_insert(0) += 1;
                }
            }
        }
    }
    if let Some(items) = mech.get("inventory").and_then(Value::as_array) {
        for item in items {
            if item.get("ComponentDefType").and_then(Value::as_str) != Some("Weapon") {
                continue;
            }
            let id = item
                .get("ComponentDefID")
                .and_then(Value::as_str)
                .unwrap_or("");
            if let Some(def) = component_value(cache, &mut comps, id) {
                let mut key = weapon_hardpoint(&def);
                let lam_categories: Vec<String> = def
                    .pointer("/Custom/Category")
                    .and_then(Value::as_array)
                    .map(|cats| {
                        cats.iter()
                            .filter_map(|c| {
                                c.get("CategoryID").and_then(Value::as_str)
                            })
                            .map(|s| s.to_ascii_lowercase())
                            .collect()
                    })
                    .unwrap_or_default();
                if lam_categories.iter().any(|c| c == "lamwingmount") {
                    key = "wingmountedweapon";
                } else if lam_categories.iter().any(|c| c == "laminternalbombbay") {
                    key = "internalbombbay";
                }
                let location = item
                    .get("MountedLocation")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if omni_locations.contains(location)
                    && matches!(key, "ballistic" | "energy" | "missile" | "antipersonnel")
                {
                    key = "universal";
                }
                if matches!(
                    key,
                    "ballistic"
                        | "energy"
                        | "missile"
                        | "universal"
                        | "antipersonnel"
                        | "wingmountedweapon"
                        | "internalbombbay"
                ) {
                    *used_hp.entry(key).or_insert(0) += 1;
                }
            }
        }
    }
    let mut hardpoints = Vec::new();
    for key in [
        "universal",
        "wingmountedweapon",
        "internalbombbay",
        "ballistic",
        "energy",
        "missile",
        "antipersonnel",
    ] {
        let max = max_hp.get(key).copied().unwrap_or(0);
        let used = used_hp.get(key).copied().unwrap_or(0);
        if max > 0 || used > 0 {
            hardpoints.push(json!({
                "key": key,
                "label": hardpoint_label(key),
                "used": used,
                "max": max,
            }));
        }
    }

    let stock_role = chassis
        .get("StockRole")
        .and_then(Value::as_str)
        .unwrap_or("");
    let description = pointer_str(&mech, &["Description", "Details"]);

    Ok(json!({
        "kind": "mech",
        "isBattleArmor": battle_armor,
        "name": name,
        "variant": variant,
        "tonnage": as_f64(&chassis, "Tonnage"),
        "weightClass": chassis.get("weightClass").and_then(Value::as_str).unwrap_or(""),
        "stockRole": stock_role,
        "description": description,
        "jumpJets": { "used": jumpjets, "max": max_jumpjets },
        "hardpoints": hardpoints,
        "armLimits": arm_limits(&chassis),
        "slotNames": {
            "specialhandheld": handheld_slot_name,
            "special": special_slot_name,
        },
        "slotIcons": {
            "specialhandheld": handheld_slot_icon,
            "special": special_slot_icon,
        },
        "slotColors": {
            "specialhandheld": handheld_slot_color,
            "special": special_slot_color,
        },
        "slotPreinstalled": {
            "specialhandheld": handheld_slot_preinstalled,
            "special": special_slot_preinstalled,
        },
        "actuatorIcons": actuator_icons,
        "ratings": ratings,
        "affinities": affinities,
        "chassis": {
            "name": chassis.get("Description").and_then(|d| d.get("Name")).and_then(Value::as_str).unwrap_or(""),
            "variant": variant,
            "tonnage": as_f64(&chassis, "Tonnage"),
            "slots": chassis_slots,
        },
        "locations": locations,
    }))
}

fn vehicle_detail(cache: &DataCache, entry: &DataEntry) -> Result<Value, String> {
    let vehicle = read_json(Path::new(&entry.path))
        .ok_or_else(|| format!("读取载具文件失败: {}", entry.path))?;
    let chassis_id = vehicle
        .get("ChassisID")
        .and_then(Value::as_str)
        .unwrap_or("");
    let chassis = cache
        .vehicle_chassis_paths
        .get(chassis_id)
        .and_then(|p| read_json(p))
        .unwrap_or(Value::Null);

    let mut comps = HashMap::new();
    let locations = build_locations(cache, &vehicle, &chassis, &mut comps);

    Ok(json!({
        "kind": "vehicle",
        "name": display_name(&vehicle, &entry.id),
        "variant": chassis.get("VariantName").and_then(Value::as_str).unwrap_or(""),
        "tonnage": as_f64(&chassis, "Tonnage"),
        "weightClass": chassis.get("weightClass").and_then(Value::as_str).unwrap_or(""),
        "topSpeed": as_f64(&chassis, "TopSpeed"),
        "movementType": chassis.get("movementType").and_then(Value::as_str).unwrap_or(""),
        "description": pointer_str(&vehicle, &["Description", "Details"]),
        "locations": locations,
    }))
}

fn component_detail(kind: &str, entry: &DataEntry) -> Result<Value, String> {
    let value = read_json(Path::new(&entry.path))
        .ok_or_else(|| format!("读取数据文件失败: {}", entry.path))?;
    let name = display_name(&value, &entry.id);
    let category = pointer_str(&value, &["Category"]);
    let subtype = pointer_str(&value, &["Type"]);
    let component_type = pointer_str(&value, &["ComponentType"]);
    let component_subtype = pointer_str(&value, &["ComponentSubType"]);
    let bonus: Vec<String> = value
        .pointer("/Custom/BonusDescriptions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let tags: Vec<String> = value
        .pointer("/ComponentTags/items")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    let subtitle = [category, subtype]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" / ");

    let slots = component_slots(&value);
    let dynamic_slots = component_dynamic_slots(&value);
    let mut fields = Vec::new();
    for (label, key) in [
        ("吨位", "Tonnage"),
        ("槽位", "InventorySize"),
        ("伤害", "Damage"),
        ("热量", "HeatGenerated"),
        ("射程", "MaxRange"),
        ("最小射程", "MinRange"),
        ("弹药", "StartingAmmoCapacity"),
        ("散热", "DissipationCapacity"),
    ] {
        if key == "InventorySize" {
            let slot_text = if dynamic_slots > 0 {
                format!("{slots}+{dynamic_slots}")
            } else {
                slots.to_string()
            };
            fields.push(json!({ "label": label, "value": slot_text }));
            continue;
        }
        let value = value.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        if value != 0.0 || key == "Tonnage" {
            fields.push(json!({ "label": label, "value": value }));
        }
    }

    Ok(json!({
        "kind": kind,
        "name": name,
        "subtitle": subtitle,
        "componentType": component_type,
        "componentSubType": component_subtype,
        "manufacturer": pointer_str(&value, &["Description", "Manufacturer"]),
        "model": pointer_str(&value, &["Description", "Model"]),
        "description": pointer_str(&value, &["Description", "Details"]),
        "fields": fields,
        "bonus": bonus,
        "tags": tags,
    }))
}

pub fn detail(cache: &DataCache, kind: &str, id: &str) -> Result<Value, String> {
    let entry = cache
        .entries
        .iter()
        .find(|entry| entry.kind == kind && entry.id == id)
        .ok_or_else(|| format!("未找到数据: {kind}/{id}"))?;
    match kind {
        "mech" => mech_detail(cache, entry),
        "vehicle" => vehicle_detail(cache, entry),
        "weapon" | "equipment" | "ammo" => component_detail(kind, entry),
        _ => Err(format!("未知数据类型: {kind}")),
    }
}
