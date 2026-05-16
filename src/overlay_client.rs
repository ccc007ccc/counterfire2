use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use bytes::BytesMut;
use core_server::ipc::protocol::ControlMessage;
use parking_lot::RwLock;
use rand::thread_rng;
use tokio::io::AsyncWriteExt;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::sync::{broadcast, mpsc, oneshot};
use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_BUSY, HANDLE};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::System::Memory::{
    FILE_MAP_ALL_ACCESS, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW,
    UnmapViewOfFile,
};
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
use windows::core::PCWSTR;

use crate::assets::{AssetCatalog, BitmapAsset};
use crate::audio::AudioPlayer;
use crate::config::AppConfig;
use crate::effects::{ActiveEffect, KillTrigger, Side};
use crate::events::{UiEvent, UiKind};

const PIPE_NAME: &str = r"\\.\pipe\overlay-core";
const SHMEM_LEN: usize = 16 * 1024 * 1024;
const CMD_CLEAR: u16 = 0x0101;
const CMD_PUSH_SPACE: u16 = 0x0109;
const CMD_POP_SPACE: u16 = 0x010A;
const CMD_DRAW_BITMAP: u16 = 0x010C;
const SPACE_ID_MONITOR_LOCAL: u32 = 1;
const INTERP_LINEAR: i32 = 1;
const RESPONSIVE_SCALE_BASE: f32 = 720.0;
const MIN_RESPONSIVE_SCALE: f32 = 0.55;
const MAX_RESPONSIVE_SCALE: f32 = 3.0;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy)]
pub struct OverlayOptions {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub vsync: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayRunExit {
    Stopped,
    CanvasSizeChanged,
}

pub struct OverlayClient {
    client: NamedPipeClient,
    shmem: SharedMemory,
    options: OverlayOptions,
    width: u32,
    height: u32,
    frame_id: u64,
    current_offset: u32,
    buf: BytesMut,
    vsync: bool,
}

impl OverlayClient {
    pub async fn connect(assets: &AssetCatalog, options: OverlayOptions) -> Result<Self> {
        let (width, height) = resolve_canvas_size(options);
        tracing::info!(width, height, "连接 overlay-engine core");

        let mut client = connect_pipe().await?;
        let mut buf = BytesMut::new();

        ControlMessage::RegisterApp {
            pid: std::process::id(),
        }
        .encode(&mut buf);
        client.write_all(&buf).await?;
        buf.clear();
        tokio::time::sleep(Duration::from_millis(500)).await;

        for bitmap in assets.bitmaps() {
            upload_bitmap(&mut client, &mut buf, bitmap).await?;
        }

        ControlMessage::CreateCanvas {
            logical_w: width,
            logical_h: height,
            render_w: width,
            render_h: height,
        }
        .encode(&mut buf);
        client.write_all(&buf).await?;
        buf.clear();

        let shmem = SharedMemory::open(std::process::id())?;
        tracing::info!(bitmap_count = assets.bitmaps().len(), "overlay 已就绪");

        Ok(Self {
            client,
            shmem,
            options,
            width,
            height,
            frame_id: 0,
            current_offset: 24,
            buf,
            vsync: options.vsync,
        })
    }

    pub async fn run(
        &mut self,
        assets: &AssetCatalog,
        rx: &mut mpsc::Receiver<KillTrigger>,
        audio: AudioPlayer,
        config: Arc<RwLock<AppConfig>>,
        shutdown: &mut oneshot::Receiver<()>,
        ui_tx: broadcast::Sender<UiEvent>,
    ) -> Result<OverlayRunExit> {
        let mut active: Option<ActiveEffect> = None;
        let mut rng = thread_rng();

        loop {
            while let Ok(trigger) = rx.try_recv() {
                let playback = config.read().clone();
                let side = playback.side.resolve(trigger.side);
                let effect = ActiveEffect::new(&trigger, &mut rng);
                audio.play_effect(assets, effect.spec(), playback.lang, side, playback.volume);
                tracing::info!(
                    event = effect.spec().key,
                    side = side_label(side),
                    "触发击杀特效"
                );
                let _ = ui_tx.send(UiEvent::info(
                    UiKind::Effect,
                    format!("触发击杀特效: {} ({})", effect.spec().key, side_label(side)),
                ));
                active = Some(effect);
            }

            let now = Instant::now();
            if active.as_ref().is_some_and(|effect| !effect.is_alive(now)) {
                active = None;
            }

            if self.canvas_size_changed() {
                let frame_config = config.read().clone();
                self.render_frame(None, assets, now, &frame_config, &mut rng)
                    .await?;
                return Ok(OverlayRunExit::CanvasSizeChanged);
            }

            let frame_config = config.read().clone();
            self.render_frame(active.as_ref(), assets, now, &frame_config, &mut rng)
                .await?;

            if self.vsync {
                if let Err(err) = unsafe { DwmFlush() } {
                    tracing::warn!(%err, "DwmFlush 失败");
                }
                tokio::select! {
                    _ = &mut *shutdown => break,
                    _ = tokio::task::yield_now() => {}
                }
            } else {
                tokio::select! {
                    _ = &mut *shutdown => break,
                    _ = tokio::time::sleep(Duration::from_millis(16)) => {}
                }
            }
        }

        let frame_config = config.read().clone();
        self.render_frame(None, assets, Instant::now(), &frame_config, &mut rng)
            .await?;
        Ok(OverlayRunExit::Stopped)
    }

    fn canvas_size_changed(&self) -> bool {
        resolve_canvas_size(self.options) != (self.width, self.height)
    }

    async fn render_frame<R: rand::Rng + ?Sized>(
        &mut self,
        active: Option<&ActiveEffect>,
        assets: &AssetCatalog,
        now: Instant,
        frame_config: &AppConfig,
        rng: &mut R,
    ) -> Result<()> {
        self.frame_id += 1;
        let cmd_offset = self.current_offset;
        let shmem = self.shmem.bytes_mut();
        let mut writer = CommandWriter::new(shmem, cmd_offset as usize);

        writer.clear(0.0, 0.0, 0.0, 0.0)?;

        if let Some(effect) = active {
            let cx = self.width as f32 * frame_config.icon_x;
            let cy = self.height as f32 * frame_config.icon_y;
            let responsive_scale = (self.width.min(self.height) as f32 / RESPONSIVE_SCALE_BASE)
                .clamp(MIN_RESPONSIVE_SCALE, MAX_RESPONSIVE_SCALE);
            let final_scale = frame_config.icon_scale
                * frame_config.icon_scales.for_spec(effect.spec())
                * responsive_scale;
            for sprite in effect.sprites(now, assets, cx, cy, final_scale, rng) {
                writer.draw_bitmap(
                    sprite.bitmap_id,
                    (0.0, 0.0, sprite.src_w, sprite.src_h),
                    (sprite.dst_x, sprite.dst_y, sprite.dst_w, sprite.dst_h),
                    sprite.opacity,
                    INTERP_LINEAR,
                )?;
            }
        }

        writer.push_space(SPACE_ID_MONITOR_LOCAL)?;
        writer.clear(0.0, 0.0, 0.0, 0.0)?;
        writer.pop_space()?;
        let cmd_length = (writer.pos() - cmd_offset as usize) as u32;

        self.current_offset += 64 * 1024;
        if self.current_offset >= 14 * 1024 * 1024 {
            self.current_offset = 24;
        }

        self.buf.clear();
        ControlMessage::SubmitFrame {
            canvas_id: 0,
            frame_id: self.frame_id,
            offset: cmd_offset,
            length: cmd_length,
        }
        .encode(&mut self.buf);
        self.client.write_all(&self.buf).await?;
        Ok(())
    }
}

fn resolve_canvas_size(options: OverlayOptions) -> (u32, u32) {
    let (screen_width, screen_height) = default_canvas_size();
    (
        options.width.unwrap_or(screen_width).max(1),
        options.height.unwrap_or(screen_height).max(1),
    )
}

fn default_canvas_size() -> (u32, u32) {
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1) as u32;
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(1) as u32;
    (width, height)
}

fn side_label(side: Option<Side>) -> &'static str {
    match side {
        Some(side) => side.suffix(),
        None => "自动/未知",
    }
}

async fn connect_pipe() -> Result<NamedPipeClient> {
    match tokio::time::timeout(CONNECT_TIMEOUT, connect_pipe_inner()).await {
        Ok(result) => result,
        Err(_) => Err(anyhow!(
            "连接 overlay-engine named pipe 超时，请先启动 overlay-engine 并打开 Xbox Game Bar 小组件"
        )),
    }
}

async fn connect_pipe_inner() -> Result<NamedPipeClient> {
    loop {
        match ClientOptions::new().open(PIPE_NAME) {
            Ok(client) => return Ok(client),
            Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY.0 as i32) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(err) => {
                return Err(err).context(
                    "连接 overlay-engine named pipe 失败，请先启动 overlay-engine 并打开 Xbox Game Bar 小组件",
                );
            }
        }
    }
}

async fn upload_bitmap(
    client: &mut NamedPipeClient,
    buf: &mut BytesMut,
    bitmap: &BitmapAsset,
) -> Result<()> {
    ControlMessage::LoadBitmap {
        bitmap_id: bitmap.id,
        bytes: bitmap.bytes.clone(),
    }
    .encode(buf);
    client.write_all(buf).await?;
    buf.clear();
    tracing::debug!(id = bitmap.id, name = bitmap.name, "已上传 bitmap");
    Ok(())
}

struct CommandWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> CommandWriter<'a> {
    fn new(buf: &'a mut [u8], pos: usize) -> Self {
        Self { buf, pos }
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let end = self
            .pos
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow!("共享内存命令偏移溢出"))?;
        if end > self.buf.len() {
            return Err(anyhow!("共享内存命令缓冲区不足"));
        }
        self.buf[self.pos..end].copy_from_slice(bytes);
        self.pos = end;
        Ok(())
    }

    fn write_u16(&mut self, value: u16) -> Result<()> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_u32(&mut self, value: u32) -> Result<()> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_i32(&mut self, value: i32) -> Result<()> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_f32(&mut self, value: f32) -> Result<()> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn clear(&mut self, r: f32, g: f32, b: f32, a: f32) -> Result<()> {
        self.write_u16(CMD_CLEAR)?;
        self.write_u16(16)?;
        self.write_f32(r)?;
        self.write_f32(g)?;
        self.write_f32(b)?;
        self.write_f32(a)
    }

    fn push_space(&mut self, space_id: u32) -> Result<()> {
        self.write_u16(CMD_PUSH_SPACE)?;
        self.write_u16(4)?;
        self.write_u32(space_id)
    }

    fn pop_space(&mut self) -> Result<()> {
        self.write_u16(CMD_POP_SPACE)?;
        self.write_u16(0)
    }

    fn draw_bitmap(
        &mut self,
        bitmap_id: u32,
        src: (f32, f32, f32, f32),
        dst: (f32, f32, f32, f32),
        opacity: f32,
        interp_mode: i32,
    ) -> Result<()> {
        self.write_u16(CMD_DRAW_BITMAP)?;
        self.write_u16(44)?;
        self.write_u32(bitmap_id)?;
        self.write_f32(src.0)?;
        self.write_f32(src.1)?;
        self.write_f32(src.2)?;
        self.write_f32(src.3)?;
        self.write_f32(dst.0)?;
        self.write_f32(dst.1)?;
        self.write_f32(dst.2)?;
        self.write_f32(dst.3)?;
        self.write_f32(opacity)?;
        self.write_i32(interp_mode)
    }
}

struct SharedMemory {
    handle: HANDLE,
    view: MEMORY_MAPPED_VIEW_ADDRESS,
}

impl SharedMemory {
    fn open(pid: u32) -> Result<Self> {
        let name = format!("overlay-core-cmds-{pid}");
        let name_w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe {
            OpenFileMappingW(FILE_MAP_ALL_ACCESS.0, false, PCWSTR(name_w.as_ptr()))
                .with_context(|| format!("打开共享内存失败: {name}"))?
        };
        let view = unsafe { MapViewOfFile(handle, FILE_MAP_ALL_ACCESS, 0, 0, 0) };
        if view.Value.is_null() {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(anyhow!("映射共享内存失败: {name}"));
        }
        Ok(Self { handle, view })
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.view.Value as *mut u8, SHMEM_LEN) }
    }
}

impl Drop for SharedMemory {
    fn drop(&mut self) {
        unsafe {
            if !self.view.Value.is_null() {
                let _ = UnmapViewOfFile(self.view);
            }
            let _ = CloseHandle(self.handle);
        }
    }
}
