use std::str::FromStr;
use std::time::Instant;

use anyhow::{Result, bail};
use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::assets::AssetCatalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang {
    #[serde(rename = "cn")]
    Cn,
    #[serde(rename = "en")]
    En,
}

impl FromStr for Lang {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "cn" | "zh" | "zh-cn" => Ok(Self::Cn),
            "en" | "english" => Ok(Self::En),
            _ => bail!("不支持的语言: {s}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    #[serde(rename = "CT")]
    Ct,
    #[serde(rename = "T")]
    T,
}

impl Side {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Ct => "CT",
            Self::T => "T",
        }
    }
}

impl FromStr for Side {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_ascii_uppercase().as_str() {
            "CT" => Ok(Self::Ct),
            "T" => Ok(Self::T),
            _ => bail!("不支持的阵营: {s}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct KillTrigger {
    pub round_kills: u32,
    pub weapon: Option<String>,
    pub is_headshot: bool,
    pub side: Option<Side>,
}

#[derive(Debug, Clone, Copy)]
pub struct EventSpec {
    pub key: &'static str,
    pub badge_pool: &'static [&'static str],
    pub voice_key: Option<&'static str>,
    pub hit_sfx: Option<&'static str>,
    pub fx_name: Option<&'static str>,
    pub no_scale: bool,
}

#[derive(Debug, Clone)]
pub struct ActiveEffect {
    spec: EventSpec,
    badge_name: &'static str,
    start: Instant,
}

#[derive(Debug, Clone)]
pub struct BitmapSprite {
    pub bitmap_id: u32,
    pub src_w: f32,
    pub src_h: f32,
    pub dst_x: f32,
    pub dst_y: f32,
    pub dst_w: f32,
    pub dst_h: f32,
    pub opacity: f32,
}

const BASE_IN_MS: f32 = 100.0;
const BASE_HOLD_MS: f32 = 1100.0;
const BASE_OUT_MS: f32 = 220.0;
const FX_DELAY_MS: f32 = BASE_IN_MS + 20.0;
const FX_START_S: f32 = 1.0;
const FX_PEAK_S: f32 = FX_START_S * 1.75;
const FX_RISE_MS: f32 = 80.0;
const FX_RETRACT_MS: f32 = 70.0;
const FX_FADE_MS: f32 = 225.0;
const TREMOR_MS: f32 = 320.0;
const TREMOR_AMP: f32 = 16.0;
const TOTAL_MS: f32 = BASE_IN_MS + BASE_HOLD_MS + BASE_OUT_MS;

const BADGE_MULTI1: &[&str] = &["badge_multi1"];
const BADGE_MULTI2: &[&str] = &["badge_multi2"];
const BADGE_MULTI3: &[&str] = &["badge_multi3"];
const BADGE_MULTI4: &[&str] = &["badge_multi4"];
const BADGE_MULTI5: &[&str] = &["badge_multi5"];
const BADGE_MULTI6: &[&str] = &["badge_multi6"];
const BADGE_HEADSHOT: &[&str] = &["badge_headshot", "badge_headshot_gold"];
const BADGE_KNIFE: &[&str] = &["badge_knife"];
const BADGE_GRENADE: &[&str] = &["badge_grenade"];

const SPEC_SINGLE: EventSpec = EventSpec {
    key: "Single",
    badge_pool: BADGE_MULTI1,
    voice_key: None,
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: None,
    no_scale: false,
};
const SPEC_DOUBLE: EventSpec = EventSpec {
    key: "Double",
    badge_pool: BADGE_MULTI2,
    voice_key: Some("MultiKill_2"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: Some("multi2_fx"),
    no_scale: false,
};
const SPEC_TRIPLE: EventSpec = EventSpec {
    key: "Triple",
    badge_pool: BADGE_MULTI3,
    voice_key: Some("MultiKill_3"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: Some("multi3_fx"),
    no_scale: false,
};
const SPEC_QUAD: EventSpec = EventSpec {
    key: "Quad",
    badge_pool: BADGE_MULTI4,
    voice_key: Some("MultiKill_4"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: Some("multi4_fx"),
    no_scale: false,
};
const SPEC_PENTA: EventSpec = EventSpec {
    key: "Penta",
    badge_pool: BADGE_MULTI5,
    voice_key: Some("MultiKill_5"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: Some("multi5_fx"),
    no_scale: false,
};
const SPEC_HEXA: EventSpec = EventSpec {
    key: "Hexa",
    badge_pool: BADGE_MULTI6,
    voice_key: Some("MultiKill_6"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: Some("multi6_fx"),
    no_scale: false,
};
const SPEC_SEPTA: EventSpec = EventSpec {
    key: "Septa",
    badge_pool: BADGE_MULTI6,
    voice_key: Some("MultiKill_7"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: Some("multi6_fx"),
    no_scale: false,
};
const SPEC_OCTO: EventSpec = EventSpec {
    key: "Octo",
    badge_pool: BADGE_MULTI6,
    voice_key: Some("MultiKill_8"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: Some("multi6_fx"),
    no_scale: false,
};
const SPEC_HEADSHOT: EventSpec = EventSpec {
    key: "Headshot",
    badge_pool: BADGE_HEADSHOT,
    voice_key: Some("Headshot"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: None,
    no_scale: true,
};
const SPEC_KNIFE: EventSpec = EventSpec {
    key: "Knife",
    badge_pool: BADGE_KNIFE,
    voice_key: Some("Knifekill"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: None,
    no_scale: false,
};
const SPEC_GRENADE: EventSpec = EventSpec {
    key: "Grenade",
    badge_pool: BADGE_GRENADE,
    voice_key: Some("Grenadekill"),
    hit_sfx: Some("UI_SPECIALKILL2"),
    fx_name: None,
    no_scale: false,
};

impl EventSpec {
    pub fn scale_key(&self) -> &'static str {
        match self.key {
            "Single" => "single",
            "Double" => "double",
            "Triple" => "triple",
            "Quad" => "quad",
            "Penta" => "penta",
            "Hexa" => "hexa",
            "Septa" => "septa",
            "Octo" => "octo",
            "Headshot" => "headshot",
            "Knife" => "knife",
            "Grenade" => "grenade",
            _ => "single",
        }
    }
}

impl ActiveEffect {
    pub fn new<R: Rng + ?Sized>(trigger: &KillTrigger, rng: &mut R) -> Self {
        let spec = spec_for_trigger(trigger);
        let badge_name = *spec.badge_pool.choose(rng).unwrap_or(&spec.badge_pool[0]);
        Self {
            spec,
            badge_name,
            start: Instant::now(),
        }
    }

    pub fn spec(&self) -> &EventSpec {
        &self.spec
    }

    pub fn is_alive(&self, now: Instant) -> bool {
        elapsed_ms(self.start, now) < TOTAL_MS
    }

    pub fn sprites<R: Rng + ?Sized>(
        &self,
        now: Instant,
        assets: &AssetCatalog,
        cx: f32,
        cy: f32,
        icon_scale: f32,
        rng: &mut R,
    ) -> Vec<BitmapSprite> {
        let elapsed = elapsed_ms(self.start, now);
        let (ox, oy) = tremor(elapsed, rng);
        let mut sprites = Vec::with_capacity(2);

        let base_opacity = base_alpha(elapsed);
        if base_opacity > 0.0
            && let Some(asset) = assets.bitmap(self.badge_name)
        {
            let scale = base_scale(elapsed, self.spec.no_scale) * icon_scale;
            sprites.push(centered_sprite(
                asset.id,
                asset.width,
                asset.height,
                cx + ox,
                cy + oy,
                scale,
                base_opacity,
            ));
        }

        if let Some((scale, opacity)) = fx_state(elapsed)
            && let Some(fx_name) = self.spec.fx_name
            && let Some(asset) = assets.bitmap(fx_name)
        {
            sprites.push(centered_sprite(
                asset.id,
                asset.width,
                asset.height,
                cx + ox,
                cy + oy,
                scale * icon_scale,
                opacity,
            ));
        }

        sprites
    }
}

fn spec_for_trigger(trigger: &KillTrigger) -> EventSpec {
    if trigger.weapon.as_deref().is_some_and(is_knife) {
        return SPEC_KNIFE;
    }
    if trigger.weapon.as_deref().is_some_and(is_grenade) {
        return SPEC_GRENADE;
    }
    if trigger.is_headshot {
        return SPEC_HEADSHOT;
    }

    match trigger.round_kills.clamp(1, 8) {
        1 => SPEC_SINGLE,
        2 => SPEC_DOUBLE,
        3 => SPEC_TRIPLE,
        4 => SPEC_QUAD,
        5 => SPEC_PENTA,
        6 => SPEC_HEXA,
        7 => SPEC_SEPTA,
        _ => SPEC_OCTO,
    }
}

fn is_knife(weapon: &str) -> bool {
    let weapon = weapon.to_ascii_lowercase();
    weapon.contains("knife") || weapon.contains("bayonet")
}

fn is_grenade(weapon: &str) -> bool {
    let weapon = weapon.to_ascii_lowercase();
    weapon.contains("hegrenade") || weapon.contains("grenade")
}

fn elapsed_ms(start: Instant, now: Instant) -> f32 {
    now.duration_since(start).as_secs_f32() * 1000.0
}

fn tremor<R: Rng + ?Sized>(elapsed: f32, rng: &mut R) -> (f32, f32) {
    let rel = elapsed - BASE_IN_MS;
    if !(0.0..TREMOR_MS).contains(&rel) {
        return (0.0, 0.0);
    }
    let amp = TREMOR_AMP * (1.0 - rel / TREMOR_MS);
    (rng.gen_range(-amp..=amp), rng.gen_range(-amp..=amp))
}

fn base_scale(elapsed: f32, no_scale: bool) -> f32 {
    if no_scale || elapsed >= BASE_IN_MS {
        1.0
    } else {
        4.0 - 3.0 * ease_out_quint(elapsed / BASE_IN_MS)
    }
}

fn base_alpha(elapsed: f32) -> f32 {
    let out_start = BASE_IN_MS + BASE_HOLD_MS;
    if elapsed < out_start {
        1.0
    } else {
        let t = ((elapsed - out_start) / BASE_OUT_MS).clamp(0.0, 1.0);
        1.0 - t
    }
}

fn fx_state(elapsed: f32) -> Option<(f32, f32)> {
    let e = elapsed - FX_DELAY_MS;
    if e < 0.0 {
        return None;
    }
    if e < FX_RISE_MS {
        let t = e / FX_RISE_MS;
        let scale = FX_START_S + (FX_PEAK_S - FX_START_S) * (1.0 - (1.0 - t).powi(4));
        return Some((scale, 1.0));
    }

    let e2 = e - FX_RISE_MS;
    if e2 >= FX_FADE_MS {
        return None;
    }

    let scale = if e2 < FX_RETRACT_MS {
        let t = e2 / FX_RETRACT_MS;
        FX_PEAK_S - (FX_PEAK_S - FX_START_S) * (1.0 - (1.0 - t).powi(4))
    } else {
        FX_START_S
    };
    Some((scale, 1.0 - e2 / FX_FADE_MS))
}

fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(5)
}

fn centered_sprite(
    bitmap_id: u32,
    width: u32,
    height: u32,
    cx: f32,
    cy: f32,
    scale: f32,
    opacity: f32,
) -> BitmapSprite {
    let dst_w = width as f32 * scale;
    let dst_h = height as f32 * scale;
    BitmapSprite {
        bitmap_id,
        src_w: width as f32,
        src_h: height as f32,
        dst_x: cx - dst_w * 0.5,
        dst_y: cy - dst_h * 0.5,
        dst_w,
        dst_h,
        opacity: opacity.clamp(0.0, 1.0),
    }
}
