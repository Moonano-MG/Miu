//! Frame driver: advances animations and renders a scene through the software
//! renderer.
//!
//! [`Player`] is the smallest complete runtime Miu offers: load a document, advance
//! time, render a frame. A game loop or editor takes the same pieces and drives them
//! itself — the player exists so tools, tests, and the CLI share one code path.

use std::collections::HashMap;

use miu_core::{QualityPreset, QualitySettings, Vec2};
use miu_graphics::{
    Assets, DisplayList, ImageError, RenderError, RenderStats, SoftwareRenderer,
};

use crate::animation::Animation;
use crate::document::{Document, DocumentError};
use crate::scene::{Scene, SceneError};

/// Failure while driving a scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerError {
    /// The document could not be loaded.
    Document(DocumentError),
    /// Rendering failed.
    Render(RenderError),
    /// PNG encoding failed.
    Image(ImageError),
    /// The scene rejected an edit.
    Scene(SceneError),
}

impl std::error::Error for PlayerError {}

impl core::fmt::Display for PlayerError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Document(error) => write!(formatter, "{error}"),
            Self::Render(error) => write!(formatter, "{error}"),
            Self::Image(error) => write!(formatter, "{error}"),
            Self::Scene(error) => write!(formatter, "{error}"),
        }
    }
}

impl From<DocumentError> for PlayerError {
    fn from(error: DocumentError) -> Self {
        Self::Document(error)
    }
}

impl From<RenderError> for PlayerError {
    fn from(error: RenderError) -> Self {
        Self::Render(error)
    }
}

impl From<ImageError> for PlayerError {
    fn from(error: ImageError) -> Self {
        Self::Image(error)
    }
}

impl From<SceneError> for PlayerError {
    fn from(error: SceneError) -> Self {
        Self::Scene(error)
    }
}

/// Scene, assets, animations, and playback time.
#[derive(Debug, Clone)]
pub struct Player {
    scene: Scene,
    assets: Assets,
    animations: Vec<Animation>,
    time: f32,
    preset: QualityPreset,
}

impl Player {
    /// Creates a player around an existing scene and asset set.
    pub fn new(scene: Scene, assets: Assets) -> Self {
        Self {
            scene,
            assets,
            animations: Vec::new(),
            time: 0.0,
            preset: QualityPreset::Medium,
        }
    }

    /// Loads a document, decoding its assets from `files` keyed by declared path.
    pub fn from_document(
        document: &Document,
        files: &HashMap<String, Vec<u8>>,
    ) -> Result<Self, PlayerError> {
        let instantiated = document.instantiate(files)?;
        let mut player = Self::new(instantiated.scene, instantiated.assets);
        player.animations = instantiated.animations;
        Ok(player)
    }

    /// Scene being played.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Mutable scene.
    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }

    /// Assets used by the scene.
    pub fn assets(&self) -> &Assets {
        &self.assets
    }

    /// Mutable assets.
    pub fn assets_mut(&mut self) -> &mut Assets {
        &mut self.assets
    }

    /// Animations applied every frame.
    pub fn animations(&self) -> &[Animation] {
        &self.animations
    }

    /// Mutable animations.
    pub fn animations_mut(&mut self) -> &mut Vec<Animation> {
        &mut self.animations
    }

    /// Playback time in seconds.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Quality preset used when the player allocates a renderer.
    pub fn preset(&self) -> QualityPreset {
        self.preset
    }

    /// Changes the quality preset.
    pub fn set_preset(&mut self, preset: QualityPreset) {
        self.preset = preset;
    }

    /// Longest animation duration, or zero when nothing is animated.
    pub fn duration(&self) -> f32 {
        self.animations
            .iter()
            .map(|animation| animation.duration)
            .fold(0.0f32, f32::max)
    }

    /// Number of animations that loop forever.
    pub fn looping_animations(&self) -> usize {
        self.animations.iter().filter(|animation| animation.looping).count()
    }

    /// Advances playback, applies animations, and returns the new time.
    pub fn advance(&mut self, delta_seconds: f32) -> Result<f32, PlayerError> {
        if delta_seconds.is_finite() && delta_seconds > 0.0 {
            self.time += delta_seconds;
        }
        self.apply_animations()?;
        Ok(self.time)
    }

    /// Jumps to an absolute time and applies animations.
    pub fn seek(&mut self, time: f32) -> Result<(), PlayerError> {
        self.time = if time.is_finite() { time.max(0.0) } else { 0.0 };
        self.apply_animations()
    }

    /// Restarts playback at zero.
    pub fn restart(&mut self) -> Result<(), PlayerError> {
        self.seek(0.0)
    }

    /// Builds the frame's display list.
    pub fn build_frame(&self) -> DisplayList {
        self.scene.build(&self.assets)
    }

    /// Scene size in whole pixels.
    pub fn size(&self) -> (u32, u32) {
        let viewport = self.scene.viewport();
        (
            viewport.x.round().clamp(1.0, 16_384.0) as u32,
            viewport.y.round().clamp(1.0, 16_384.0) as u32,
        )
    }

    /// Renders into a caller-owned renderer, which must already hold the assets.
    ///
    /// Use [`Player::upload_assets`] to copy them when the renderer is new.
    pub fn render_into(&self, renderer: &mut SoftwareRenderer) -> Result<RenderStats, RenderError> {
        let list = self.build_frame();
        let camera = *self.scene.camera();
        renderer.begin_frame(self.scene.background())?;
        renderer.draw(&camera, &list)?;
        Ok(renderer.stats())
    }

    /// Copies the player's assets into a renderer.
    pub fn upload_assets(&self, renderer: &mut SoftwareRenderer) {
        *renderer.assets_mut() = self.assets.clone();
    }

    /// Creates a renderer sized to the scene and uploads the assets.
    pub fn create_renderer(&self) -> Result<SoftwareRenderer, PlayerError> {
        let (width, height) = self.size();
        let mut renderer =
            SoftwareRenderer::with_settings(width, height, QualitySettings::for_preset(self.preset))?;
        self.upload_assets(&mut renderer);
        Ok(renderer)
    }

    /// Renders one frame and encodes it as PNG.
    pub fn render_frame(&self) -> Result<Vec<u8>, PlayerError> {
        let (width, height) = self.size();
        let mut renderer =
            SoftwareRenderer::with_settings(width, height, QualitySettings::for_preset(self.preset))?;
        self.upload_assets(&mut renderer);
        self.render_into(&mut renderer)?;
        Ok(renderer.encode_png()?)
    }

    /// Renders one frame into an already allocated framebuffer size, cropping or
    /// padding against the scene viewport.
    pub fn render_at_size(&self, width: u32, height: u32) -> Result<Vec<u8>, PlayerError> {
        let mut renderer = SoftwareRenderer::with_settings(
            width.max(1),
            height.max(1),
            QualitySettings::for_preset(self.preset),
        )?;
        self.upload_assets(&mut renderer);
        self.render_into(&mut renderer)?;
        Ok(renderer.encode_png()?)
    }

    fn apply_animations(&mut self) -> Result<(), PlayerError> {
        let time = self.time;
        for animation in &self.animations {
            animation.apply(&mut self.scene, time)?;
        }
        Ok(())
    }
}

/// Size of a square thumbnail used by asset browsers.
pub const THUMBNAIL_SIZE: Vec2 = Vec2::new(160.0, 90.0);
