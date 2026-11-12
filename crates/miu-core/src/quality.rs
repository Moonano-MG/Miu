//! Quality presets and the adaptive quality governor.
//!
//! Each renderer tier exposes four presets. A preset is a *budget*: the share of
//! frame time an application is willing to trade for image quality.
//!
//! | Preset | Performance | Quality |
//! |---|---|---|
//! | `Low` | 90% | 10% |
//! | `Medium` | 70% | 30% |
//! | `High` | 40% | 60% |
//! | `Unlimited` | 10% | 90% |
//!
//! [`QualitySettings`] turns a preset into concrete switches for both the software
//! renderer and GPU bridges. [`QualityGovernor`] implements runtime adaptation: it
//! watches frame times and steps the preset down when the host cannot keep up, then
//! steps back up when there is headroom.

use crate::policy::RenderTier;

/// Quality/performance preset selected by the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum QualityPreset {
    /// Maximum frame rate, minimal effects.
    Low = 1,
    /// Balanced preset for mainstream devices.
    Medium = 2,
    /// Favours image quality while keeping interaction smooth.
    High = 3,
    /// Best available image quality; frame time is not capped.
    Unlimited = 4,
}

impl QualityPreset {
    /// Converts a C ABI discriminant into a preset.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Low),
            2 => Some(Self::Medium),
            3 => Some(Self::High),
            4 => Some(Self::Unlimited),
            _ => None,
        }
    }

    /// Stable identifier used by configuration files, logs, and telemetry.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Unlimited => "unlimited",
        }
    }

    /// Performance/quality split published for this preset.
    pub const fn budget(self) -> PresetBudget {
        match self {
            Self::Low => PresetBudget { performance_percent: 90, quality_percent: 10 },
            Self::Medium => PresetBudget { performance_percent: 70, quality_percent: 30 },
            Self::High => PresetBudget { performance_percent: 40, quality_percent: 60 },
            Self::Unlimited => PresetBudget { performance_percent: 10, quality_percent: 90 },
        }
    }

    /// Concrete renderer switches for this preset.
    pub const fn settings(self) -> QualitySettings {
        QualitySettings::for_preset(self)
    }

    /// The next weaker preset, if any.
    pub const fn lower(self) -> Option<Self> {
        match self {
            Self::Low => None,
            Self::Medium => Some(Self::Low),
            Self::High => Some(Self::Medium),
            Self::Unlimited => Some(Self::High),
        }
    }

    /// The next stronger preset, if any.
    pub const fn higher(self) -> Option<Self> {
        match self {
            Self::Low => Some(Self::Medium),
            Self::Medium => Some(Self::High),
            Self::High => Some(Self::Unlimited),
            Self::Unlimited => None,
        }
    }

    /// Clamps a preset to the maximum a tier can honour.
    pub const fn clamp_to_tier(self, tier: RenderTier) -> Self {
        let maximum = tier.max_preset();
        if self as u8 > maximum as u8 {
            maximum
        } else {
            self
        }
    }
}

/// Published performance/quality split of a preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresetBudget {
    /// Share of the frame budget spent on throughput.
    pub performance_percent: u8,
    /// Share of the frame budget spent on image quality.
    pub quality_percent: u8,
}

/// Renderer switches implied by a preset.
///
/// GPU bridges read these to choose sample counts, filtering, and effect passes;
/// the software renderer uses them for dithering and glyph rasterisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualitySettings {
    /// Preset these settings came from.
    pub preset: QualityPreset,
    /// Multisample count for GPU render targets; `1` means no MSAA.
    pub msaa_samples: u8,
    /// Use bilinear sampling for scaled sprites and gradients.
    pub bilinear_sampling: bool,
    /// Dither gradients to hide 8-bit banding.
    pub gradient_dithering: bool,
    /// Position glyphs at fractional pixel offsets (GPU text quality).
    pub subpixel_text: bool,
    /// Allow expensive effects such as blur and multi-pass compositing.
    pub allow_effects: bool,
    /// Maximum number of painter layers a frame may use.
    pub max_layers: u32,
    /// Maximum sprites submitted per frame before the renderer drops the excess.
    pub max_sprites_per_frame: u32,
    /// Budget for cached glyph coverage masks, in bytes.
    pub glyph_cache_bytes: u64,
    /// Budget for decoded texture data, in bytes.
    pub texture_cache_bytes: u64,
}

impl QualitySettings {
    /// Returns the settings for a preset.
    pub const fn for_preset(preset: QualityPreset) -> Self {
        match preset {
            QualityPreset::Low => Self {
                preset,
                msaa_samples: 1,
                bilinear_sampling: false,
                gradient_dithering: false,
                subpixel_text: false,
                allow_effects: false,
                max_layers: 32,
                max_sprites_per_frame: 4_096,
                glyph_cache_bytes: 2 * 1024 * 1024,
                texture_cache_bytes: 64 * 1024 * 1024,
            },
            QualityPreset::Medium => Self {
                preset,
                msaa_samples: 2,
                bilinear_sampling: true,
                gradient_dithering: false,
                subpixel_text: false,
                allow_effects: false,
                max_layers: 64,
                max_sprites_per_frame: 16_384,
                glyph_cache_bytes: 8 * 1024 * 1024,
                texture_cache_bytes: 128 * 1024 * 1024,
            },
            QualityPreset::High => Self {
                preset,
                msaa_samples: 4,
                bilinear_sampling: true,
                gradient_dithering: true,
                subpixel_text: true,
                allow_effects: true,
                max_layers: 128,
                max_sprites_per_frame: 65_536,
                glyph_cache_bytes: 16 * 1024 * 1024,
                texture_cache_bytes: 256 * 1024 * 1024,
            },
            QualityPreset::Unlimited => Self {
                preset,
                msaa_samples: 8,
                bilinear_sampling: true,
                gradient_dithering: true,
                subpixel_text: true,
                allow_effects: true,
                max_layers: 512,
                max_sprites_per_frame: 1_048_576,
                glyph_cache_bytes: 64 * 1024 * 1024,
                texture_cache_bytes: 1024 * 1024 * 1024,
            },
        }
    }

    /// Whether this preset enables expensive post-processing.
    pub const fn uses_effects(&self) -> bool {
        self.allow_effects
    }
}

impl Default for QualitySettings {
    fn default() -> Self {
        Self::for_preset(QualityPreset::Medium)
    }
}

/// Adaptive preset controller driven by measured frame times.
///
/// Typical use is one call per rendered frame:
///
/// ```ignore
/// if let Some(preset) = governor.record_frame(frame_seconds) {
///     renderer.set_preset(preset);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct QualityGovernor {
    target_frame_seconds: f32,
    smoothed_frame_seconds: f32,
    preset: QualityPreset,
    lowest: QualityPreset,
    highest: QualityPreset,
    over_budget_frames: u32,
    under_budget_frames: u32,
    cooldown_frames: u32,
    downsample_threshold: f32,
    upsample_threshold: f32,
    required_over_frames: u32,
    required_under_frames: u32,
    cooldown_length: u32,
}

impl QualityGovernor {
    /// Creates a governor targeting `target_fps` frames per second.
    ///
    /// The preset range defaults to `Low ..= Unlimited` so a 60 FPS target can
    /// recover quality on fast hardware.
    pub fn new(target_fps: f32, initial: QualityPreset) -> Self {
        let fps = if target_fps.is_finite() && target_fps > 0.0 { target_fps } else { 60.0 };
        Self {
            target_frame_seconds: 1.0 / fps,
            smoothed_frame_seconds: 1.0 / fps,
            preset: initial,
            lowest: QualityPreset::Low,
            highest: QualityPreset::Unlimited,
            over_budget_frames: 0,
            under_budget_frames: 0,
            cooldown_frames: 0,
            downsample_threshold: 1.15,
            upsample_threshold: 0.75,
            required_over_frames: 30,
            required_under_frames: 120,
            cooldown_length: 90,
        }
    }

    /// Restricts the presets the governor may choose.
    #[must_use]
    pub fn with_range(mut self, lowest: QualityPreset, highest: QualityPreset) -> Self {
        let (low, high) = if lowest <= highest { (lowest, highest) } else { (highest, lowest) };
        self.lowest = low;
        self.highest = high;
        self.preset = if self.preset < low {
            low
        } else if self.preset > high {
            high
        } else {
            self.preset
        };
        self
    }

    /// Overrides the frame thresholds and hysteresis lengths; mostly for tests.
    #[must_use]
    pub fn with_tuning(
        mut self,
        downsample_threshold: f32,
        upsample_threshold: f32,
        required_over_frames: u32,
        required_under_frames: u32,
        cooldown_length: u32,
    ) -> Self {
        self.downsample_threshold = downsample_threshold;
        self.upsample_threshold = upsample_threshold;
        self.required_over_frames = required_over_frames.max(1);
        self.required_under_frames = required_under_frames.max(1);
        self.cooldown_length = cooldown_length;
        self
    }

    /// Preset currently in force.
    pub fn preset(&self) -> QualityPreset {
        self.preset
    }

    /// Settings for the preset currently in force.
    pub fn settings(&self) -> QualitySettings {
        QualitySettings::for_preset(self.preset)
    }

    /// Smoothed frame duration in seconds.
    pub fn smoothed_frame_seconds(&self) -> f32 {
        self.smoothed_frame_seconds
    }

    /// Target frame duration in seconds.
    pub fn target_frame_seconds(&self) -> f32 {
        self.target_frame_seconds
    }

    /// Frames still blocked from changing the preset.
    pub fn cooldown_frames(&self) -> u32 {
        self.cooldown_frames
    }

    /// Feeds one measured frame duration and returns the new preset when it changed.
    ///
    /// The measurement is smoothed with an exponential moving average so a single
    /// slow frame (asset upload, garbage collection) cannot trigger a change.
    pub fn record_frame(&mut self, frame_seconds: f32) -> Option<QualityPreset> {
        if !frame_seconds.is_finite() || frame_seconds <= 0.0 {
            return None;
        }
        const SMOOTHING: f32 = 0.1;
        self.smoothed_frame_seconds += (frame_seconds - self.smoothed_frame_seconds) * SMOOTHING;
        if self.cooldown_frames > 0 {
            self.cooldown_frames -= 1;
            return None;
        }
        let smoothed = self.smoothed_frame_seconds;
        if smoothed > self.target_frame_seconds * self.downsample_threshold {
            self.over_budget_frames = self.over_budget_frames.saturating_add(1);
            self.under_budget_frames = 0;
        } else if smoothed < self.target_frame_seconds * self.upsample_threshold {
            self.under_budget_frames = self.under_budget_frames.saturating_add(1);
            self.over_budget_frames = 0;
        } else {
            self.over_budget_frames = 0;
            self.under_budget_frames = 0;
            return None;
        }

        if self.over_budget_frames >= self.required_over_frames {
            if let Some(lower) = self.preset.lower() {
                if lower >= self.lowest {
                    self.preset = lower;
                    self.reset_counters();
                    return Some(self.preset);
                }
            }
            self.over_budget_frames = 0;
        } else if self.under_budget_frames >= self.required_under_frames {
            if let Some(higher) = self.preset.higher() {
                if higher <= self.highest {
                    self.preset = higher;
                    self.reset_counters();
                    return Some(self.preset);
                }
            }
            self.under_budget_frames = 0;
        }
        None
    }

    fn reset_counters(&mut self) {
        self.over_budget_frames = 0;
        self.under_budget_frames = 0;
        self.cooldown_frames = self.cooldown_length;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_publish_the_documented_split() {
        assert_eq!(
            QualityPreset::Low.budget(),
            PresetBudget { performance_percent: 90, quality_percent: 10 }
        );
        assert_eq!(
            QualityPreset::Unlimited.budget(),
            PresetBudget { performance_percent: 10, quality_percent: 90 }
        );
    }

    #[test]
    fn tiers_clamp_presets() {
        assert_eq!(QualityPreset::Unlimited.clamp_to_tier(RenderTier::Compatibility), QualityPreset::Medium);
        assert_eq!(QualityPreset::Unlimited.clamp_to_tier(RenderTier::Balanced), QualityPreset::High);
        assert_eq!(QualityPreset::Low.clamp_to_tier(RenderTier::Premium), QualityPreset::Low);
    }

    #[test]
    fn settings_grow_with_preset() {
        let low = QualitySettings::for_preset(QualityPreset::Low);
        let unlimited = QualitySettings::for_preset(QualityPreset::Unlimited);
        assert!(low.msaa_samples < unlimited.msaa_samples);
        assert!(!low.allow_effects && unlimited.allow_effects);
        assert!(low.max_sprites_per_frame < unlimited.max_sprites_per_frame);
    }

    #[test]
    fn governor_drops_quality_under_sustained_load() {
        let mut governor = QualityGovernor::new(60.0, QualityPreset::High)
            .with_range(QualityPreset::Low, QualityPreset::Unlimited);
        let mut change = None;
        for _ in 0..200 {
            if let Some(preset) = governor.record_frame(1.0 / 30.0) {
                change = Some(preset);
                break;
            }
        }
        assert_eq!(change, Some(QualityPreset::Medium));
    }

    #[test]
    fn governor_recovers_quality_with_headroom() {
        let mut governor = QualityGovernor::new(60.0, QualityPreset::Low)
            .with_range(QualityPreset::Low, QualityPreset::High);
        let mut change = None;
        for _ in 0..600 {
            if let Some(preset) = governor.record_frame(1.0 / 240.0) {
                change = Some(preset);
                break;
            }
        }
        assert_eq!(change, Some(QualityPreset::Medium));
    }

    #[test]
    fn governor_respects_its_upper_bound() {
        let mut governor = QualityGovernor::new(60.0, QualityPreset::High)
            .with_range(QualityPreset::Low, QualityPreset::High);
        for _ in 0..2000 {
            governor.record_frame(1.0 / 1000.0);
        }
        assert_eq!(governor.preset(), QualityPreset::High);
    }

    #[test]
    fn invalid_measurements_are_ignored() {
        let mut governor = QualityGovernor::new(60.0, QualityPreset::Medium);
        assert_eq!(governor.record_frame(0.0), None);
        assert_eq!(governor.record_frame(f32::NAN), None);
        assert_eq!(governor.preset(), QualityPreset::Medium);
    }
}
