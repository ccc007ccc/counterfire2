use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::effects::{Lang, Side};

const MAX_PNG_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_PNG_BYTES: u64 = 64 * 1024 * 1024;
const MAX_OGG_BYTES: u64 = 16 * 1024 * 1024;
const MAX_BITMAP_DIMENSION: u32 = 8192;

#[derive(Debug, Clone)]
pub struct BitmapAsset {
    pub id: u32,
    pub name: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct AssetCatalog {
    root: PathBuf,
    bitmaps: Vec<BitmapAsset>,
    by_name: HashMap<String, usize>,
    bitmap_bytes: u64,
}

impl AssetCatalog {
    pub fn load(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        let mut catalog = Self {
            root,
            bitmaps: Vec::new(),
            by_name: HashMap::new(),
            bitmap_bytes: 0,
        };
        catalog.load_png_dir("badges")?;
        catalog.load_png_dir("fx")?;
        catalog.validate_sound_dirs()?;
        Ok(catalog)
    }

    pub fn bitmaps(&self) -> &[BitmapAsset] {
        &self.bitmaps
    }

    pub fn bitmap(&self, name: &str) -> Option<&BitmapAsset> {
        self.by_name.get(name).map(|&idx| &self.bitmaps[idx])
    }

    pub fn voice_path(&self, lang: Lang, side: Side, voice_key: &str) -> Option<PathBuf> {
        let dir = match lang {
            Lang::Cn => "snd_cn",
            Lang::En => "snd_en",
        };
        self.existing_sound(dir, &format!("{}_{}.ogg", voice_key, side.suffix()))
    }

    pub fn hit_sfx_path(&self, hit_sfx: &str) -> Option<PathBuf> {
        let file = format!("{hit_sfx}.ogg");
        self.existing_sound("snd_hit", &file)
            .or_else(|| self.existing_sound("snd_gen", &file))
    }

    fn load_png_dir(&mut self, dir: &str) -> Result<()> {
        let full_dir = self.root.join(dir);
        let mut paths = std::fs::read_dir(&full_dir)
            .with_context(|| format!("读取素材目录失败: {}", full_dir.display()))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.retain(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        });
        paths.sort();

        for path in paths {
            let name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .context("素材文件名不是有效 UTF-8")?
                .to_owned();
            if self.by_name.contains_key(&name) {
                bail!("重复的 bitmap 素材名: {name}");
            }
            let size = std::fs::metadata(&path)
                .with_context(|| format!("读取 PNG 元数据失败: {}", path.display()))?
                .len();
            if size > MAX_PNG_BYTES {
                bail!("PNG 素材过大: {}", path.display());
            }
            self.bitmap_bytes = self.bitmap_bytes.saturating_add(size);
            if self.bitmap_bytes > MAX_TOTAL_PNG_BYTES {
                bail!("PNG 素材总大小超过上限: {}", self.root.display());
            }
            let bytes = std::fs::read(&path)
                .with_context(|| format!("读取 PNG 素材失败: {}", path.display()))?;
            let (width, height) = image::image_dimensions(&path)
                .with_context(|| format!("读取 PNG 尺寸失败: {}", path.display()))?;
            if width > MAX_BITMAP_DIMENSION || height > MAX_BITMAP_DIMENSION {
                bail!("PNG 素材尺寸过大: {}", path.display());
            }
            let id = self.bitmaps.len() as u32 + 1;
            self.by_name.insert(name.clone(), self.bitmaps.len());
            self.bitmaps.push(BitmapAsset {
                id,
                name,
                bytes,
                width,
                height,
            });
        }

        Ok(())
    }

    fn validate_sound_dirs(&self) -> Result<()> {
        for dir in ["snd_cn", "snd_en", "snd_gen", "snd_hit"] {
            let full_dir = self.root.join(dir);
            if !full_dir.exists() {
                continue;
            }
            for entry in std::fs::read_dir(&full_dir)
                .with_context(|| format!("读取音频素材目录失败: {}", full_dir.display()))?
            {
                let path = entry?.path();
                let is_ogg = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ogg"));
                if !is_ogg {
                    continue;
                }
                let size = std::fs::metadata(&path)
                    .with_context(|| format!("读取 OGG 元数据失败: {}", path.display()))?
                    .len();
                if size > MAX_OGG_BYTES {
                    bail!("OGG 素材过大: {}", path.display());
                }
            }
        }
        Ok(())
    }

    fn existing_sound(&self, dir: &str, file: &str) -> Option<PathBuf> {
        let path = self.root.join(dir).join(file);
        Path::new(&path).exists().then_some(path)
    }
}
