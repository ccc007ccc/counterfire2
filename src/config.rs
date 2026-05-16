use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
};
use windows::core::PCWSTR;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

use crate::effects::{EventSpec, Lang, Side};

pub const MIN_ICON_SCALE: f32 = 0.5;
pub const MAX_ICON_SCALE: f32 = 2.0;
pub const MIN_ICON_POSITION: f32 = 0.0;
pub const MAX_ICON_POSITION: f32 = 1.0;
pub const DEFAULT_ICON_X: f32 = 0.5;
pub const DEFAULT_ICON_Y: f32 = 0.5;
pub const MIN_VOLUME: f32 = 0.0;
pub const MAX_VOLUME: f32 = 2.0;
pub const MIN_KILL_STREAK_RESET_SECONDS: f32 = 1.0;
pub const MAX_KILL_STREAK_RESET_SECONDS: f32 = 120.0;
pub const DEFAULT_KILL_STREAK_RESET_SECONDS: f32 = 15.0;
pub const DEFAULT_GSI_PORT: u16 = 57534;
pub const MAX_CANVAS_DIMENSION: u32 = 16384;
const CONFIG_FILE_NAME: &str = "config.json";
const INVALID_CONFIG_FILE_NAME: &str = "config.invalid.json";
const TEMP_CONFIG_FILE_NAME: &str = "config.json.tmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunMode {
    Gsi,
    Demo,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SidePreference {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "CT")]
    Ct,
    #[serde(rename = "T")]
    T,
}

impl SidePreference {
    pub fn resolve(self, detected: Option<Side>) -> Option<Side> {
        match self {
            Self::Auto => detected,
            Self::Ct => Some(Side::Ct),
            Self::T => Some(Side::T),
        }
    }
}

impl FromStr for SidePreference {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "ct" => Ok(Self::Ct),
            "t" => Ok(Self::T),
            _ => bail!("未知阵营: {value}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IconScales {
    pub single: f32,
    pub double: f32,
    pub triple: f32,
    pub quad: f32,
    pub penta: f32,
    pub hexa: f32,
    pub septa: f32,
    pub octo: f32,
    pub headshot: f32,
    pub knife: f32,
    pub grenade: f32,
}

impl Default for IconScales {
    fn default() -> Self {
        Self {
            single: 1.0,
            double: 1.0,
            triple: 1.0,
            quad: 1.0,
            penta: 1.0,
            hexa: 1.0,
            septa: 1.0,
            octo: 1.0,
            headshot: 1.0,
            knife: 1.0,
            grenade: 1.0,
        }
    }
}

impl IconScales {
    pub fn for_spec(&self, spec: &EventSpec) -> f32 {
        match spec.scale_key() {
            "single" => self.single,
            "double" => self.double,
            "triple" => self.triple,
            "quad" => self.quad,
            "penta" => self.penta,
            "hexa" => self.hexa,
            "septa" => self.septa,
            "octo" => self.octo,
            "headshot" => self.headshot,
            "knife" => self.knife,
            "grenade" => self.grenade,
            _ => 1.0,
        }
    }

    fn normalize(&mut self) {
        self.single = normalize_icon_scale(self.single);
        self.double = normalize_icon_scale(self.double);
        self.triple = normalize_icon_scale(self.triple);
        self.quad = normalize_icon_scale(self.quad);
        self.penta = normalize_icon_scale(self.penta);
        self.hexa = normalize_icon_scale(self.hexa);
        self.septa = normalize_icon_scale(self.septa);
        self.octo = normalize_icon_scale(self.octo);
        self.headshot = normalize_icon_scale(self.headshot);
        self.knife = normalize_icon_scale(self.knife);
        self.grenade = normalize_icon_scale(self.grenade);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub mode: RunMode,
    pub port: u16,
    pub lang: Lang,
    pub side: SidePreference,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub vsync: bool,
    pub icon_scale: f32,
    pub icon_scales: IconScales,
    pub icon_x: f32,
    pub icon_y: f32,
    pub volume: f32,
    pub kill_streak_reset_seconds: f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mode: RunMode::Gsi,
            port: DEFAULT_GSI_PORT,
            lang: Lang::Cn,
            side: SidePreference::Auto,
            width: None,
            height: None,
            vsync: true,
            icon_scale: 1.0,
            icon_scales: IconScales::default(),
            icon_x: DEFAULT_ICON_X,
            icon_y: DEFAULT_ICON_Y,
            volume: 1.0,
            kill_streak_reset_seconds: DEFAULT_KILL_STREAK_RESET_SECONDS,
        }
    }
}

impl AppConfig {
    pub fn load_from(dir: &Path) -> Result<Self> {
        let path = dir.join(CONFIG_FILE_NAME);
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(mut config) => {
                    config.normalize();
                    Ok(config)
                }
                Err(err) => {
                    preserve_invalid_config(&path);
                    tracing::warn!(%err, path = %path.display(), "配置文件无效，已回退默认配置");
                    Ok(Self::default())
                }
            },
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err).with_context(|| format!("读取配置失败: {}", path.display())),
        }
    }

    pub fn save_to(&self, dir: &Path) -> Result<PathBuf> {
        fs::create_dir_all(dir).with_context(|| format!("创建配置目录失败: {}", dir.display()))?;
        let path = dir.join(CONFIG_FILE_NAME);
        let temp_path = dir.join(TEMP_CONFIG_FILE_NAME);
        {
            let mut file = File::create(&temp_path)
                .with_context(|| format!("创建临时配置失败: {}", temp_path.display()))?;
            serde_json::to_writer_pretty(&mut file, self)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
        }
        replace_file(&temp_path, &path)
            .with_context(|| format!("写入配置失败: {}", path.display()))?;
        Ok(path)
    }

    pub fn normalize(&mut self) {
        self.width = normalize_dimension(self.width);
        self.height = normalize_dimension(self.height);
        self.icon_scale = normalize_icon_scale(self.icon_scale);
        self.icon_scales.normalize();
        self.icon_x = clamp_f32(
            self.icon_x,
            MIN_ICON_POSITION,
            MAX_ICON_POSITION,
            DEFAULT_ICON_X,
        );
        self.icon_y = clamp_f32(
            self.icon_y,
            MIN_ICON_POSITION,
            MAX_ICON_POSITION,
            DEFAULT_ICON_Y,
        );
        self.volume = clamp_f32(self.volume, MIN_VOLUME, MAX_VOLUME, 1.0);
        self.kill_streak_reset_seconds = clamp_f32(
            self.kill_streak_reset_seconds,
            MIN_KILL_STREAK_RESET_SECONDS,
            MAX_KILL_STREAK_RESET_SECONDS,
            DEFAULT_KILL_STREAK_RESET_SECONDS,
        );
    }
}

pub enum CliAction {
    Run(AppConfig),
    Help,
}

pub fn parse_cli_args(args: impl IntoIterator<Item = String>) -> Result<CliAction> {
    let mut config = AppConfig::default();
    let mut args = args.into_iter().skip(1).filter(|arg| arg != "--cli");

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => config.mode = RunMode::Demo,
            "--port" => config.port = next_value(&mut args, "--port")?.parse()?,
            "--lang" => config.lang = Lang::from_str(&next_value(&mut args, "--lang")?)?,
            "--side" => config.side = SidePreference::from_str(&next_value(&mut args, "--side")?)?,
            "--width" => config.width = Some(next_value(&mut args, "--width")?.parse()?),
            "--height" => config.height = Some(next_value(&mut args, "--height")?.parse()?),
            "--icon-scale" => config.icon_scale = next_value(&mut args, "--icon-scale")?.parse()?,
            "--icon-x" => config.icon_x = next_value(&mut args, "--icon-x")?.parse()?,
            "--icon-y" => config.icon_y = next_value(&mut args, "--icon-y")?.parse()?,
            "--volume" => config.volume = next_value(&mut args, "--volume")?.parse()?,
            "--kill-streak-reset" => {
                config.kill_streak_reset_seconds =
                    next_value(&mut args, "--kill-streak-reset")?.parse()?
            }
            "--no-vsync" | "--unlocked" => config.vsync = false,
            "--help" | "-h" => return Ok(CliAction::Help),
            _ => bail!("未知参数: {arg}"),
        }
    }

    config.normalize();
    Ok(CliAction::Run(config))
}

pub fn help_text() -> &'static str {
    "CounterFire 2\n\n\
     用法:\n\
     counterfire2                 打开窗口控制面板\n\
     counterfire2 --cli [--demo] [--port 57534] [--lang cn|en] [--side auto|CT|T] [--width px] [--height px] [--icon-scale 1.0] [--icon-x 0.5] [--icon-y 0.5] [--volume 1.0] [--kill-streak-reset 15] [--no-vsync]\n\n\
     --cli       使用控制台 fallback 模式\n\
     --demo      不等待 CS2 GSI，循环触发示例击杀特效\n\
     --port      CS2 GSI HTTP 监听端口，默认 57534\n\
     --lang      语音语言，默认 cn\n\
     --side      阵营选择，auto 为 GSI 自动识别，默认 auto\n\
     --width     Game Bar 画布宽度覆盖，不传则使用当前屏幕宽度\n\
     --height    Game Bar 画布高度覆盖，不传则使用当前屏幕高度\n\
     --icon-scale 击杀图标大小倍率，范围 0.5-2.0，默认 1.0\n\
     --icon-x    击杀图标水平位置，范围 0.0-1.0，默认 0.5\n\
     --icon-y    击杀图标垂直位置，范围 0.0-1.0，默认 0.5\n\
     --volume    音效音量倍率，范围 0.0-2.0，默认 1.0\n\
     --kill-streak-reset 连杀重置秒数，范围 1-120，默认 15\n\
     --no-vsync  不调用 DwmFlush，改为约 60 FPS 定时"
}

fn normalize_icon_scale(value: f32) -> f32 {
    clamp_f32(value, MIN_ICON_SCALE, MAX_ICON_SCALE, 1.0)
}

fn normalize_dimension(value: Option<u32>) -> Option<u32> {
    value.filter(|value| (1..=MAX_CANVAS_DIMENSION).contains(value))
}

fn preserve_invalid_config(path: &Path) {
    let invalid_path = path.with_file_name(INVALID_CONFIG_FILE_NAME);
    let _ = fs::remove_file(&invalid_path);
    let _ = fs::rename(path, invalid_path);
}

fn replace_file(from: &Path, to: &Path) -> Result<()> {
    let from_w = path_to_wide(from);
    let to_w = path_to_wide(to);
    unsafe {
        MoveFileExW(
            PCWSTR(from_w.as_ptr()),
            PCWSTR(to_w.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .context("替换配置文件失败")?;
    Ok(())
}

fn path_to_wide(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn clamp_f32(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| anyhow::anyhow!("{flag} 缺少参数"))
}
