//! Skin catalog adapted from xiaobo121388's mcdev-tools PR #10 (MIT).
use crate::mcdk_config::{bounded_read, invalid};
use crate::{LocalIndex, Result, mcdk::McdkStore};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

const PREVIEW_LIMIT: usize = 8 * 1024 * 1024;
const CATALOG_LIMIT: usize = 1024 * 1024;
const ARCHIVE_LIMIT: usize = 2 * 1024 * 1024;
// Only these upstream support archives are accepted; never extract ZIPs to disk.
const NETEASE: &[(&str, &str, &str)] = &[
    ("v2025s5_ceshi", "4680777111329095206.zip", "4D测试皮肤"),
    (
        "v2024s4_cs_s2blackhole_skin",
        "4674169033125275215.zip",
        "黑洞之心",
    ),
    (
        "xy_s6airplane_skin",
        "4682037015921590802.zip",
        "4D云霄战姬",
    ),
    ("xvniouxianga", "4673037477981775941.zip", "4D学生晴音"),
];

#[derive(Debug, Serialize)]
pub struct Skin {
    pub name: String,
    pub path: PathBuf,
    pub texture: String,
    pub slim: bool,
    pub kind: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Catalog {
    pub skins: Vec<Skin>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSkin {
    pub path: String,
    pub slim: bool,
}

pub fn catalog(game: &Path) -> Result<Catalog> {
    if !game.is_file() || !extension(game, "exe") {
        return Err(invalid("请先选择有效的游戏 EXE 文件"));
    }
    let game_dir = game.parent().ok_or_else(|| invalid("游戏路径无效"))?;
    let mut result = Catalog {
        skins: vec![],
        warnings: vec![],
    };
    let vanilla = game_dir.join("data/skin_packs/vanilla");
    match vanilla_catalog(&vanilla) {
        Ok(skins) => result.skins.extend(skins),
        Err(error) => result.warnings.push(format!("原版皮肤：{error}")),
    }
    let support = game_dir.join("../../../componentcache/support");
    if let Ok(root) = crate::canonicalize_path(&support) {
        for (directory, archive, name) in NETEASE {
            let path = support.join(directory).join(archive);
            if let Ok(real) = crate::canonicalize_path(&path) {
                if real.starts_with(&root) && preview(&real).is_ok() {
                    result.skins.push(Skin {
                        name: name.to_string(),
                        path: real,
                        texture: format!("{directory}/{archive}"),
                        slim: false,
                        kind: "netease4d",
                    });
                }
            }
        }
    }
    Ok(result)
}

fn vanilla_catalog(directory: &Path) -> Result<Vec<Skin>> {
    let root = crate::canonicalize_path(directory)?;
    let bytes = bounded_read(&root.join("skins.json"), CATALOG_LIMIT)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid("skins.json 不是 UTF-8"))?;
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| invalid("skins.json 无效"))?;
    let entries = value["skins"]
        .as_array()
        .filter(|v| v.len() <= 256)
        .ok_or_else(|| invalid("皮肤目录无效或超过 256 项"))?;
    let mut seen = HashSet::new();
    let mut budget = 16 * 1024 * 1024;
    let mut skins = vec![];
    for entry in entries {
        let (Some(name), Some(texture), Some(geometry)) = (
            entry["localization_name"].as_str(),
            entry["texture"].as_str(),
            entry["geometry"].as_str(),
        ) else {
            continue;
        };
        if entry["type"] != "free"
            || name.trim().is_empty()
            || !matches!(
                geometry,
                "geometry.humanoid.custom" | "geometry.humanoid.customSlim"
            )
        {
            continue;
        }
        let relative = PathBuf::from(texture.replace('\\', "/"));
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
            || !extension(&relative, "png")
        {
            continue;
        }
        let Ok(real) = crate::canonicalize_path(&root.join(&relative)) else {
            continue;
        };
        if !real.starts_with(&root) || !seen.insert(real.to_string_lossy().to_lowercase()) {
            continue;
        }
        let Ok(bytes) = bounded_read(&real, PREVIEW_LIMIT) else {
            continue;
        };
        if bytes.len() > budget {
            return Err(invalid("皮肤目录读取量超过 16 MiB"));
        }
        budget -= bytes.len();
        if validate_png(&bytes).is_err() {
            continue;
        }
        skins.push(Skin {
            name: name.into(),
            path: real,
            texture: texture.replace('\\', "/"),
            slim: geometry.ends_with("Slim"),
            kind: "vanilla",
        });
    }
    Ok(skins)
}

pub fn preview(path: &Path) -> Result<String> {
    let bytes = if extension(path, "png") {
        bounded_read(path, PREVIEW_LIMIT)?
    } else if extension(path, "zip") {
        archive_preview(path)?
    } else {
        return Err(invalid("只支持 PNG 或已知网易 4D 皮肤包"));
    };
    validate_png(&bytes)?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}

fn archive_preview(path: &Path) -> Result<Vec<u8>> {
    let descriptor = NETEASE
        .iter()
        .find(|(dir, archive, _)| {
            path.file_name()
                .is_some_and(|v| v.to_string_lossy().eq_ignore_ascii_case(archive))
                && path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|v| v.to_string_lossy().eq_ignore_ascii_case(dir))
        })
        .ok_or_else(|| invalid("不是已知的网易 4D 皮肤包"))?;
    let bytes = bounded_read(path, ARCHIVE_LIMIT)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| invalid("4D ZIP 无效"))?;
    if zip.len() > 1024 {
        return Err(invalid("4D ZIP 条目过多"));
    }
    let expected = format!("{0}/{0}.png", descriptor.0);
    let mut found = None;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|_| invalid("ZIP 条目不可读"))?;
        if entry
            .name()
            .replace('\\', "/")
            .trim_start_matches("./")
            .eq_ignore_ascii_case(&expected)
        {
            if found.is_some() || entry.size() > PREVIEW_LIMIT as u64 || entry.is_dir() {
                return Err(invalid("ZIP 预览条目重复或过大"));
            }
            let mut bytes = vec![];
            entry
                .take(PREVIEW_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| invalid("ZIP 预览损坏"))?;
            if bytes.len() > PREVIEW_LIMIT {
                return Err(invalid("ZIP 预览过大"));
            }
            found = Some(bytes);
        }
    }
    found.ok_or_else(|| invalid("4D 包缺少固定预览图"))
}

fn validate_png(bytes: &[u8]) -> Result<()> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(png::Limits {
        bytes: 32 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|_| invalid("PNG 头部无效"))?;
    let info = reader.info();
    if info.width == 0
        || info.height == 0
        || info.width > 2048
        || info.height > 2048
        || info.animation_control.is_some()
    {
        return Err(invalid("PNG 尺寸超过 2048 或为动画图像"));
    }
    let size = reader
        .output_buffer_size()
        .filter(|v| *v <= 32 * 1024 * 1024)
        .ok_or_else(|| invalid("PNG 解码内存过大"))?;
    reader
        .next_frame(&mut vec![0; size])
        .map_err(|_| invalid("PNG 图像数据无效"))?;
    reader.finish().map_err(|_| invalid("PNG 数据不完整"))?;
    Ok(())
}

fn extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .is_some_and(|v| v.to_string_lossy().eq_ignore_ascii_case(expected))
}

pub fn library(index: &LocalIndex, component_id: &str) -> Result<Vec<CustomSkin>> {
    let store = McdkStore::new(index.clone());
    store.launch_target(component_id)?;
    store.read(&format!("skin-library:{component_id}"))
}

pub fn update_library(
    index: &LocalIndex,
    component_id: &str,
    path: &str,
    slim: bool,
    remove: bool,
) -> Result<Vec<CustomSkin>> {
    let _guard = index.try_lock_mutations()?;
    let store = McdkStore::new(index.clone());
    let root = store.launch_target(component_id)?.path;
    if path.trim().is_empty() || path.len() > 32768 {
        return Err(invalid("皮肤路径无效"));
    }
    let mut items = library(index, component_id)?;
    let requested = if remove {
        path.to_owned()
    } else {
        let full = root.join(path);
        if !extension(&full, "png") {
            return Err(invalid("自定义皮肤库只接受 PNG"));
        }
        let real = crate::canonicalize_path(&full)?;
        preview(&real)?;
        real.to_string_lossy().into_owned()
    };
    let normalize = |s: &str| s.replace('\\', "/").to_lowercase();
    items.retain(|v| normalize(&v.path) != normalize(&requested));
    if !remove {
        if items.len() >= 64 {
            return Err(invalid("每个组件最多保存 64 个自定义皮肤"));
        }
        items.push(CustomSkin {
            path: requested,
            slim,
        });
    }
    store.write(&format!("skin-library:{component_id}"), &items)?;
    Ok(items)
}

#[cfg(test)]
mod tests;
