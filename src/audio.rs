use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;

use crate::assets::AssetCatalog;
use crate::effects::{EventSpec, Lang, Side};

const MAX_AUDIO_THREADS: usize = 8;

#[derive(Debug, Clone)]
pub struct AudioPlayer {
    active_threads: Arc<AtomicUsize>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer {
    pub fn new() -> Self {
        Self {
            active_threads: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn play_effect(
        &self,
        assets: &AssetCatalog,
        spec: &EventSpec,
        lang: Lang,
        side: Option<Side>,
        volume: f32,
    ) {
        if let (Some(voice_key), Some(side)) = (spec.voice_key, side)
            && let Some(path) = assets.voice_path(lang, side, voice_key)
        {
            self.play_path(path, volume);
        }
        if let Some(hit_sfx) = spec.hit_sfx
            && let Some(path) = assets.hit_sfx_path(hit_sfx)
        {
            self.play_path(path, volume);
        }
    }

    fn play_path(&self, path: PathBuf, volume: f32) {
        if !self.acquire_thread_slot() {
            eprintln!("[counterfire2] 音频播放过于频繁，已丢弃 {}", path.display());
            return;
        }
        let active_threads = Arc::clone(&self.active_threads);
        std::thread::spawn(move || {
            if let Err(err) = play_blocking(&path, volume) {
                eprintln!("[counterfire2] 音频播放失败 {}: {err}", path.display());
            }
            active_threads.fetch_sub(1, Ordering::AcqRel);
        });
    }

    fn acquire_thread_slot(&self) -> bool {
        let mut current = self.active_threads.load(Ordering::Acquire);
        loop {
            if current >= MAX_AUDIO_THREADS {
                return false;
            }
            match self.active_threads.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(next) => current = next,
            }
        }
    }
}

fn play_blocking(path: &Path, volume: f32) -> Result<()> {
    let mut stream_handle = rodio::DeviceSinkBuilder::open_default_sink()?;
    stream_handle.log_on_drop(false);
    let file = File::open(path)?;
    let player = rodio::play(stream_handle.mixer(), BufReader::new(file))?;
    player.set_volume(volume);
    player.sleep_until_end();
    Ok(())
}
