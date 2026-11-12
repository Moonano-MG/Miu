//! Scene document format: a JSON description of a scene, its assets, and animations.
//!
//! Documents are the interchange format between the engine, tools, and a visual-novel
//! editor. [`Document::parse`] validates structure; [`Document::instantiate`] decodes
//! the referenced assets from caller-supplied bytes and builds a [`Scene`].
//!
//! ```json
//! {
//!   "version": 1,
//!   "viewport": [1280, 720],
//!   "background": "#101018",
//!   "camera": { "position": [0, 0], "zoom": 1.0 },
//!   "assets": {
//!     "textures": { "hero": "art/hero.png" },
//!     "fonts": { "body": "fonts/Body.ttf" }
//!   },
//!   "nodes": [
//!     { "type": "sprite", "name": "hero", "texture": "hero",
//!       "size": [256, 512], "pivot": [0.5, 1.0], "position": [640, 700] },
//!     { "type": "text", "name": "line", "font": "body", "text": "Hello",
//!       "size": 28, "color": "#ffffff", "position": [80, 600] }
//!   ],
//!   "animations": [
//!     { "name": "idle", "duration": 2, "loop": true,
//!       "tracks": [ { "node": "hero", "property": "y",
//!                      "keys": [[0, 700, "ease_in_out"], [1, 690], [2, 700]] } ] }
//!   ]
//! }
//! ```
//!
//! Angles in documents are degrees; the engine's own API uses radians.

use std::collections::HashMap;

use miu_core::{Affine2, BlendMode, CameraError, Color, Rect, Rgba8, Transform2D, Vec2};
use miu_graphics::{
    Assets, Font, GradientStop, LineCap, LineJoin, Paint, PaintError, Path, Spread, StrokeStyle,
    TextAlign, TextStyle, TextWrap,
};

use crate::animation::{Animation, Easing, Keyframe, NodeProperty, Track};
use crate::json::{JsonError, JsonValue, parse_hex_color};
use crate::node::{Node, NodeId, NodeKind, ShapeVisual, SpriteVisual, TextVisual, ROOT};
use crate::scene::{Scene, SceneError};

/// Failure while reading or instantiating a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    /// The JSON syntax is invalid.
    Json(JsonError),
    /// The document uses a feature this loader does not implement.
    Unsupported(String),
    /// A required field is missing or has the wrong type.
    InvalidField(String),
    /// An asset was referenced but its bytes were not supplied.
    MissingAsset(String),
    /// An asset could not be decoded.
    Decode { path: String, message: String },
    /// An animation referenced a node name that does not exist.
    UnknownNode(String),
    /// The scene rejected a node.
    Scene(SceneError),
    /// The camera configuration is invalid.
    Camera(CameraError),
}

impl std::error::Error for DocumentError {}

impl core::fmt::Display for DocumentError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Unsupported(what) => write!(formatter, "unsupported document feature: {what}"),
            Self::InvalidField(field) => write!(formatter, "invalid document field: {field}"),
            Self::MissingAsset(path) => write!(formatter, "asset data not supplied for {path}"),
            Self::Decode { path, message } => write!(formatter, "cannot decode {path}: {message}"),
            Self::UnknownNode(name) => write!(formatter, "animation targets unknown node {name}"),
            Self::Scene(error) => write!(formatter, "scene error: {error}"),
            Self::Camera(error) => write!(formatter, "camera error: {}", error.id()),
        }
    }
}

impl From<JsonError> for DocumentError {
    fn from(error: JsonError) -> Self {
        Self::Json(error)
    }
}

impl From<SceneError> for DocumentError {
    fn from(error: SceneError) -> Self {
        Self::Scene(error)
    }
}

impl From<CameraError> for DocumentError {
    fn from(error: CameraError) -> Self {
        Self::Camera(error)
    }
}

/// Asset category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// Encoded image (PNG, BMP, TGA, or QOI).
    Texture,
    /// TrueType font.
    Font,
}

/// Asset declared by a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetRef {
    /// Runtime id assigned to the asset.
    pub id: u64,
    /// Name used by nodes inside the document.
    pub name: String,
    /// Path the host must resolve.
    pub path: String,
    /// Whether the file is an image or a font.
    pub kind: AssetKind,
}

/// Authored sprite settings.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteSpec {
    /// Texture asset id.
    pub texture: u64,
    /// Atlas source rectangle.
    pub source: miu_graphics::SourceRect,
    /// Destination size; zero means natural size.
    pub size: Vec2,
    /// Origin inside the destination rectangle.
    pub pivot: Vec2,
    /// Tint colour.
    pub color: Color,
    /// Whether sampling is mirrored horizontally.
    pub flip_x: bool,
    /// Whether sampling is mirrored vertically.
    pub flip_y: bool,
}

/// Authored text settings.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpec {
    /// Font asset id.
    pub font: u64,
    /// Text content.
    pub text: String,
    /// Layout style.
    pub style: TextStyle,
    /// Text colour.
    pub color: Color,
    /// Wrap width, if any.
    pub max_width: Option<f32>,
    /// Origin inside the text block.
    pub pivot: Vec2,
}

/// Authored shape settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSpec {
    /// Outline.
    pub path: Path,
    /// Fill paint.
    pub fill: Option<Paint>,
    /// Stroke style and paint.
    pub stroke: Option<(StrokeStyle, Paint)>,
    /// Origin inside the shape bounds.
    pub pivot: Vec2,
}

/// What a document node draws.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeSpecKind {
    /// Grouping node.
    Group,
    /// Sprite node.
    Sprite(SpriteSpec),
    /// Text node.
    Text(TextSpec),
    /// Shape node.
    Shape(ShapeSpec),
}

/// Authored node with its children.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSpec {
    /// Editor-facing name.
    pub name: String,
    /// Visual payload.
    pub kind: NodeSpecKind,
    /// Local transform.
    pub transform: Transform2D,
    /// Painter layer.
    pub layer: i32,
    /// Opacity in `[0, 1]`.
    pub opacity: f32,
    /// Visibility flag.
    pub visible: bool,
    /// Blend mode.
    pub blend: BlendMode,
    /// Child nodes.
    pub children: Vec<NodeSpec>,
}

/// Authored animation track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackSpec {
    /// Target node name.
    pub node: String,
    /// Animated property.
    pub property: NodeProperty,
    /// Keyframes with their easing curves.
    pub keys: Vec<Keyframe>,
}

/// Authored animation.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationSpec {
    /// Animation name.
    pub name: String,
    /// Duration in seconds.
    pub duration: f32,
    /// Whether the animation loops.
    pub looping: bool,
    /// Tracks.
    pub tracks: Vec<TrackSpec>,
}

/// Parsed scene document.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// Format version; currently `1`.
    pub version: u32,
    /// Viewport size in logical pixels.
    pub viewport: Vec2,
    /// Optional clear colour.
    pub background: Option<Rgba8>,
    /// Initial camera position.
    pub camera_position: Vec2,
    /// Initial camera zoom.
    pub camera_zoom: f32,
    /// Initial camera rotation in radians.
    pub camera_rotation: f32,
    /// Declared assets.
    pub assets: Vec<AssetRef>,
    /// Root-level node specs.
    pub nodes: Vec<NodeSpec>,
    /// Animated properties.
    pub animations: Vec<AnimationSpec>,
}

/// Document instantiated into runtime objects.
#[derive(Debug, Clone)]
pub struct Instantiated {
    /// Built scene.
    pub scene: Scene,
    /// Decoded assets referenced by the scene.
    pub assets: Assets,
    /// Animations resolved to node ids.
    pub animations: Vec<Animation>,
}

/// Asset supplied for a document reference.
#[derive(Debug, Clone)]
pub enum ResolvedAsset {
    /// Decoded image.
    Texture(miu_graphics::Texture),
    /// Parsed font.
    Font(Font),
}

/// Assets declared in a document, indexed by name.
#[derive(Debug, Default)]
pub struct AssetTable {
    textures: HashMap<String, u64>,
    fonts: HashMap<String, u64>,
    next_id: u64,
}

impl AssetTable {
    fn assign(&mut self, name: &str, kind: AssetKind) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        match kind {
            AssetKind::Texture => {
                self.textures.insert(name.to_string(), id);
            }
            AssetKind::Font => {
                self.fonts.insert(name.to_string(), id);
            }
        }
        id
    }

    /// Texture id for a name.
    pub fn texture(&self, name: &str) -> Option<u64> {
        self.textures.get(name).copied()
    }

    /// Font id for a name.
    pub fn font(&self, name: &str) -> Option<u64> {
        self.fonts.get(name).copied()
    }
}

impl Document {
    /// Parses and validates a document.
    pub fn parse(text: &str) -> Result<Self, DocumentError> {
        let value = JsonValue::parse(text)?;
        if value.as_object().is_none() {
            return Err(DocumentError::InvalidField("document must be an object".into()));
        }
        let version = value.get("version").and_then(JsonValue::as_f32).unwrap_or(1.0);
        if version < 1.0 || version > 1.0 {
            return Err(DocumentError::Unsupported(format!("document version {version}")));
        }
        let viewport = value
            .get("viewport")
            .and_then(JsonValue::as_vec2)
            .ok_or_else(|| DocumentError::InvalidField("viewport".into()))?;
        let background = value.get("background").and_then(JsonValue::as_color).map(|color| color.to_rgba8());

        let mut camera_position = Vec2::ZERO;
        let mut camera_zoom = 1.0;
        let mut camera_rotation = 0.0;
        if let Some(camera) = value.get("camera") {
            if let Some(position) = camera.get("position").and_then(JsonValue::as_vec2) {
                camera_position = position;
            }
            if let Some(zoom) = camera.get("zoom").and_then(JsonValue::as_f32) {
                camera_zoom = zoom;
            }
            if let Some(rotation) = camera.get("rotation").and_then(JsonValue::as_f32) {
                camera_rotation = rotation.to_radians();
            }
        }

        let mut table = AssetTable::default();
        let mut assets = Vec::new();
        if let Some(assets_value) = value.get("assets") {
            for (section, kind) in [
                ("textures", AssetKind::Texture),
                ("fonts", AssetKind::Font),
            ] {
                if let Some(group) = assets_value.get(section).and_then(JsonValue::as_object) {
                    for (name, path_value) in group {
                        let path = path_value
                            .as_str()
                            .ok_or_else(|| DocumentError::InvalidField(format!("assets.{section}.{name}")))?;
                        let id = table.assign(name, kind);
                        assets.push(AssetRef {
                            id,
                            name: name.clone(),
                            path: path.to_string(),
                            kind,
                        });
                    }
                }
            }
        }

        let mut nodes = Vec::new();
        if let Some(array) = value.get("nodes").and_then(JsonValue::as_array) {
            for node in array {
                nodes.push(parse_node(node, &table)?);
            }
        }

        let mut animations = Vec::new();
        if let Some(array) = value.get("animations").and_then(JsonValue::as_array) {
            for animation in array {
                animations.push(parse_animation(animation)?);
            }
        }

        Ok(Self {
            version: 1,
            viewport,
            background,
            camera_position,
            camera_zoom,
            camera_rotation,
            assets,
            nodes,
            animations,
        })
    }

    /// Builds the scene from assets produced by `resolve`.
    ///
    /// Hosts that already hold decoded resources (a GPU bridge, an editor cache, or
    /// the C ABI) use this to skip re-decoding files.
    pub fn instantiate_with<F>(&self, mut resolve: F) -> Result<Instantiated, DocumentError>
    where
        F: FnMut(&AssetRef) -> Result<ResolvedAsset, DocumentError>,
    {
        let mut assets = Assets::new();
        for asset in &self.assets {
            match resolve(asset)? {
                ResolvedAsset::Texture(texture) => {
                    assets.insert_texture(asset.id, texture);
                }
                ResolvedAsset::Font(font) => {
                    assets.insert_font(asset.id, font);
                }
            }
        }
        self.build_scene(assets)
    }

    /// Decodes assets from `files` (keyed by declared path) and builds the scene.
    ///
    /// Missing entries fail with [`DocumentError::MissingAsset`], so a host can report
    /// exactly which file a player is missing instead of rendering a blank frame.
    pub fn instantiate(
        &self,
        files: &HashMap<String, Vec<u8>>,
    ) -> Result<Instantiated, DocumentError> {
        self.instantiate_with(|asset| {
            let bytes = files
                .get(&asset.path)
                .ok_or_else(|| DocumentError::MissingAsset(asset.path.clone()))?;
            match asset.kind {
                AssetKind::Texture => {
                    let image = miu_graphics::decode_image(bytes).map_err(|error| {
                        DocumentError::Decode {
                            path: asset.path.clone(),
                            message: error.to_string(),
                        }
                    })?;
                    let texture = image.to_texture().map_err(|error| DocumentError::Decode {
                        path: asset.path.clone(),
                        message: error.to_string(),
                    })?;
                    Ok(ResolvedAsset::Texture(texture))
                }
                AssetKind::Font => {
                    let font = Font::from_bytes(bytes.clone()).map_err(|error| {
                        DocumentError::Decode {
                            path: asset.path.clone(),
                            message: error.to_string(),
                        }
                    })?;
                    Ok(ResolvedAsset::Font(font))
                }
            }
        })
    }

    fn build_scene(&self, assets: Assets) -> Result<Instantiated, DocumentError> {
        let mut scene = Scene::new(self.viewport)?;
        scene.set_background(self.background);
        {
            let camera = scene.camera_mut();
            camera.position = self.camera_position;
            camera.zoom = self.camera_zoom.max(0.0001);
            camera.rotation_radians = self.camera_rotation;
            camera.validate()?;
        }

        for spec in &self.nodes {
            instantiate_node(&mut scene, ROOT, spec)?;
        }

        let mut animations = Vec::new();
        for spec in &self.animations {
            let mut animation = Animation::new(spec.name.clone(), spec.duration);
            animation.looping = spec.looping;
            for track in &spec.tracks {
                let node = scene
                    .find_by_name(&track.node)
                    .ok_or_else(|| DocumentError::UnknownNode(track.node.clone()))?;
                animation.add_track(Track::new(node, track.property, track.keys.clone()));
            }
            animations.push(animation);
        }

        Ok(Instantiated { scene, assets, animations })
    }
}

fn instantiate_node(scene: &mut Scene, parent: NodeId, spec: &NodeSpec) -> Result<NodeId, DocumentError> {
    let kind = match &spec.kind {
        NodeSpecKind::Group => NodeKind::Group,
        NodeSpecKind::Sprite(sprite) => {
            let mut visual = SpriteVisual::new(sprite.texture);
            visual.source = sprite.source;
            visual.size = sprite.size;
            visual.pivot = sprite.pivot;
            visual.color = sprite.color;
            visual.blend = spec.blend;
            if sprite.flip_x {
                visual.uv = visual.uv.flipped_x();
            }
            if sprite.flip_y {
                visual.uv = visual.uv.flipped_y();
            }
            NodeKind::Sprite(visual)
        }
        NodeSpecKind::Text(text) => {
            let mut visual = TextVisual::new(text.font, text.text.clone(), text.style.em_size());
            visual.style = text.style;
            visual.color = text.color;
            visual.max_width = text.max_width;
            visual.pivot = text.pivot;
            visual.blend = spec.blend;
            NodeKind::Text(visual)
        }
        NodeSpecKind::Shape(shape) => NodeKind::Shape(ShapeVisual {
            path: shape.path.clone(),
            fill: shape.fill.clone(),
            stroke: shape.stroke.clone(),
            blend: spec.blend,
            pivot: shape.pivot,
        }),
    };
    let mut node = Node::new(spec.name.clone(), kind);
    node.transform = spec.transform;
    node.layer = spec.layer;
    node.opacity = spec.opacity;
    node.visible = spec.visible;
    let id = scene.create(parent, node)?;
    for child in &spec.children {
        instantiate_node(scene, id, child)?;
    }
    Ok(id)
}

fn parse_node(value: &JsonValue, assets: &AssetTable) -> Result<NodeSpec, DocumentError> {
    let type_name = value
        .get("type")
        .and_then(JsonValue::as_str)
        .unwrap_or("group");
    let name = value
        .get("name")
        .and_then(JsonValue::as_str)
        .map(|text| text.to_string())
        .unwrap_or_else(|| format!("{type_name}_{}", name_suffix(value)));

    let position = value.get("position").and_then(JsonValue::as_vec2).unwrap_or(Vec2::ZERO);
    let rotation = value.get("rotation").and_then(JsonValue::as_f32).unwrap_or(0.0).to_radians();
    let scale = read_scale(value.get("scale"))?;
    let layer = value.get("layer").and_then(JsonValue::as_f32).unwrap_or(0.0) as i32;
    let opacity = value.get("opacity").and_then(JsonValue::as_f32).unwrap_or(1.0);
    let visible = value.get("visible").and_then(JsonValue::as_bool).unwrap_or(true);
    let blend = read_blend(value.get("blend"))?;

    let kind = match type_name {
        "group" => NodeSpecKind::Group,
        "sprite" => {
            let texture_name = value
                .get("texture")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| DocumentError::InvalidField(format!("{name}.texture")))?;
            let texture = assets
                .texture(texture_name)
                .ok_or_else(|| DocumentError::InvalidField(format!("{name}.texture={texture_name}")))?;
            let size = value.get("size").and_then(JsonValue::as_vec2).unwrap_or(Vec2::ZERO);
            let pivot = value.get("pivot").and_then(JsonValue::as_vec2).unwrap_or(Vec2::ZERO);
            let color = value
                .get("color")
                .and_then(JsonValue::as_color)
                .unwrap_or(Color::WHITE);
            let source = match value.get("source") {
                Some(source) => {
                    let array = source
                        .as_array()
                        .ok_or_else(|| DocumentError::InvalidField(format!("{name}.source")))?;
                    if array.len() < 4 {
                        return Err(DocumentError::InvalidField(format!("{name}.source")));
                    }
                    let component = |index: usize| -> u32 {
                        array[index].as_f32().unwrap_or(0.0).max(0.0) as u32
                    };
                    miu_graphics::SourceRect::new(
                        component(0),
                        component(1),
                        component(2),
                        component(3),
                    )
                }
                None => miu_graphics::SourceRect::FULL,
            };
            NodeSpecKind::Sprite(SpriteSpec {
                texture,
                source,
                size,
                pivot,
                color,
                flip_x: value.get("flip_x").and_then(JsonValue::as_bool).unwrap_or(false),
                flip_y: value.get("flip_y").and_then(JsonValue::as_bool).unwrap_or(false),
            })
        }
        "text" => {
            let font_name = value
                .get("font")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| DocumentError::InvalidField(format!("{name}.font")))?;
            let font = assets
                .font(font_name)
                .ok_or_else(|| DocumentError::InvalidField(format!("{name}.font={font_name}")))?;
            let text = value
                .get("text")
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_string();
            let size = value.get("size").and_then(JsonValue::as_f32).unwrap_or(16.0);
            let mut style = TextStyle::new(size);
            if let Some(line_height) = value.get("line_height").and_then(JsonValue::as_f32) {
                style = style.with_line_height(line_height);
            }
            if let Some(spacing) = value.get("letter_spacing").and_then(JsonValue::as_f32) {
                style = style.with_letter_spacing(spacing);
            }
            if let Some(align) = value.get("align").and_then(JsonValue::as_str) {
                style = style.with_align(match align {
                    "left" => TextAlign::Left,
                    "center" => TextAlign::Center,
                    "right" => TextAlign::Right,
                    other => {
                        return Err(DocumentError::Unsupported(format!("text alignment {other}")))
                    }
                });
            }
            if let Some(wrap) = value.get("wrap").and_then(JsonValue::as_str) {
                style = style.with_wrap(match wrap {
                    "none" => TextWrap::None,
                    "word" => TextWrap::Word,
                    "character" => TextWrap::Character,
                    other => return Err(DocumentError::Unsupported(format!("text wrap {other}"))),
                });
            }
            let color = value
                .get("color")
                .and_then(JsonValue::as_color)
                .unwrap_or(Color::WHITE);
            let max_width = value.get("max_width").and_then(JsonValue::as_f32);
            let pivot = value.get("pivot").and_then(JsonValue::as_vec2).unwrap_or(Vec2::ZERO);
            NodeSpecKind::Text(TextSpec { font, text, style, color, max_width, pivot })
        }
        "shape" => {
            let path = parse_shape_path(value, &name)?;
            let fill = match value.get("fill") {
                Some(fill) => parse_paint(fill)?,
                None => None,
            };
            let stroke_paint = value.get("stroke").map(parse_paint).transpose()?.flatten();
            let stroke = match stroke_paint {
                Some(paint) => {
                    let width = value.get("stroke_width").and_then(JsonValue::as_f32).unwrap_or(1.0);
                    let mut style = StrokeStyle::new(width);
                    if let Some(cap) = value.get("stroke_cap").and_then(JsonValue::as_str) {
                        style.cap = match cap {
                            "butt" => LineCap::Butt,
                            "round" => LineCap::Round,
                            "square" => LineCap::Square,
                            other => {
                                return Err(DocumentError::Unsupported(format!("line cap {other}")))
                            }
                        };
                    }
                    if let Some(join) = value.get("stroke_join").and_then(JsonValue::as_str) {
                        style.join = match join {
                            "miter" => LineJoin::Miter,
                            "round" => LineJoin::Round,
                            "bevel" => LineJoin::Bevel,
                            other => {
                                return Err(DocumentError::Unsupported(format!("line join {other}")))
                            }
                        };
                    }
                    if let Some(dash) = value.get("dash").and_then(JsonValue::as_array) {
                        let pattern: Vec<f32> =
                            dash.iter().filter_map(JsonValue::as_f32).collect();
                        style = style.with_dash(pattern, 0.0);
                    }
                    Some((style, paint))
                }
                None => None,
            };
            let pivot = value.get("pivot").and_then(JsonValue::as_vec2).unwrap_or(Vec2::ZERO);
            NodeSpecKind::Shape(ShapeSpec { path, fill, stroke, pivot })
        }
        other => return Err(DocumentError::Unsupported(format!("node type {other}"))),
    };

    let mut children = Vec::new();
    if let Some(array) = value.get("children").and_then(JsonValue::as_array) {
        for child in array {
            children.push(parse_node(child, assets)?);
        }
    }

    Ok(NodeSpec {
        name,
        kind,
        transform: Transform2D::new(position, rotation, scale),
        layer,
        opacity,
        visible,
        blend,
        children,
    })
}

fn name_suffix(value: &JsonValue) -> u32 {
    value
        .get("layer")
        .and_then(JsonValue::as_f32)
        .map(|layer| layer.abs() as u32)
        .unwrap_or(0)
}

fn read_scale(value: Option<&JsonValue>) -> Result<Vec2, DocumentError> {
    match value {
        None => Ok(Vec2::ONE),
        Some(value) => {
            if let Some(scalar) = value.as_f32() {
                return Ok(Vec2::splat(scalar));
            }
            value
                .as_vec2()
                .ok_or_else(|| DocumentError::InvalidField("scale".into()))
        }
    }
}

fn read_blend(value: Option<&JsonValue>) -> Result<BlendMode, DocumentError> {
    let Some(value) = value else {
        return Ok(BlendMode::SourceOver);
    };
    let Some(name) = value.as_str() else {
        return Err(DocumentError::InvalidField("blend".into()));
    };
    match name {
        "source_over" | "normal" => Ok(BlendMode::SourceOver),
        "add" | "additive" => Ok(BlendMode::Add),
        "multiply" => Ok(BlendMode::Multiply),
        "screen" => Ok(BlendMode::Screen),
        "copy" => Ok(BlendMode::Copy),
        other => Err(DocumentError::Unsupported(format!("blend mode {other}"))),
    }
}

fn parse_shape_path(value: &JsonValue, name: &str) -> Result<Path, DocumentError> {
    let kind = value.get("shape").and_then(JsonValue::as_str).unwrap_or("rect");
    match kind {
        "rect" | "round_rect" => {
            let rect = value
                .get("rect")
                .and_then(JsonValue::as_array)
                .ok_or_else(|| DocumentError::InvalidField(format!("{name}.rect")))?;
            if rect.len() < 4 {
                return Err(DocumentError::InvalidField(format!("{name}.rect")));
            }
            let component = |index: usize| rect[index].as_f32().unwrap_or(0.0);
            let rectangle = Rect::new(component(0), component(1), component(2), component(3));
            if kind == "rect" {
                Ok(Path::rect(rectangle))
            } else {
                let radius = value.get("radius").and_then(JsonValue::as_f32).unwrap_or(0.0);
                Ok(Path::round_rect(rectangle, radius))
            }
        }
        "ellipse" => {
            let center = value
                .get("center")
                .and_then(JsonValue::as_vec2)
                .ok_or_else(|| DocumentError::InvalidField(format!("{name}.center")))?;
            let radii = value
                .get("radii")
                .and_then(JsonValue::as_vec2)
                .ok_or_else(|| DocumentError::InvalidField(format!("{name}.radii")))?;
            Ok(Path::ellipse(center, radii))
        }
        "polygon" | "polyline" => {
            let points = value
                .get("points")
                .and_then(JsonValue::as_array)
                .ok_or_else(|| DocumentError::InvalidField(format!("{name}.points")))?;
            let mut parsed = Vec::new();
            for point in points {
                parsed.push(
                    point
                        .as_vec2()
                        .ok_or_else(|| DocumentError::InvalidField(format!("{name}.points")))?,
                );
            }
            Ok(Path::polyline(&parsed, kind == "polygon"))
        }
        other => Err(DocumentError::Unsupported(format!("shape kind {other}"))),
    }
}

fn parse_paint(value: &JsonValue) -> Result<Option<Paint>, DocumentError> {
    if value.is_null() {
        return Ok(None);
    }
    if let Some(text) = value.as_str() {
        return Ok(parse_hex_color(text).map(Paint::solid));
    }
    if value.as_array().is_some() {
        return Ok(value.as_color().map(Paint::solid));
    }
    let Some(object) = value.as_object() else {
        return Err(DocumentError::InvalidField("paint".into()));
    };
    if object.iter().any(|(name, _)| name == "linear" || name == "radial") {
        let (kind, body) = object
            .iter()
            .find(|(name, _)| name == "linear" || name == "radial")
            .ok_or_else(|| DocumentError::InvalidField("paint".into()))?;
        let from = body.get("from").and_then(JsonValue::as_vec2).unwrap_or(Vec2::ZERO);
        let to = body.get("to").and_then(JsonValue::as_vec2).unwrap_or(Vec2::new(1.0, 0.0));
        let center = body.get("center").and_then(JsonValue::as_vec2).unwrap_or(Vec2::ZERO);
        let radius = body.get("radius").and_then(JsonValue::as_f32).unwrap_or(1.0);
        let spread = match body.get("spread").and_then(JsonValue::as_str) {
            Some("repeat") => Spread::Repeat,
            Some("reflect") => Spread::Reflect,
            _ => Spread::Pad,
        };
        let stops_value = body
            .get("stops")
            .and_then(JsonValue::as_array)
            .ok_or_else(|| DocumentError::InvalidField("gradient stops".into()))?;
        let mut stops = Vec::new();
        for stop in stops_value {
            let pair = stop
                .as_array()
                .ok_or_else(|| DocumentError::InvalidField("gradient stop".into()))?;
            if pair.len() < 2 {
                return Err(DocumentError::InvalidField("gradient stop".into()));
            }
            let offset = pair[0].as_f32().unwrap_or(0.0);
            let color = pair[1]
                .as_color()
                .ok_or_else(|| DocumentError::InvalidField("gradient stop colour".into()))?;
            stops.push(GradientStop::new(offset, color));
        }
        let paint = if kind == "linear" {
            Paint::linear(from, to, stops, spread)
        } else {
            Paint::radial(center, radius, stops, spread)
        }
        .map_err(|error: PaintError| DocumentError::Unsupported(error.to_string()))?;
        return Ok(Some(paint));
    }
    Err(DocumentError::InvalidField("paint".into()))
}

fn parse_animation(value: &JsonValue) -> Result<AnimationSpec, DocumentError> {
    let name = value
        .get("name")
        .and_then(JsonValue::as_str)
        .unwrap_or("animation")
        .to_string();
    let duration = value.get("duration").and_then(JsonValue::as_f32).unwrap_or(1.0);
    let looping = value.get("loop").and_then(JsonValue::as_bool).unwrap_or(false);
    let mut tracks = Vec::new();
    if let Some(array) = value.get("tracks").and_then(JsonValue::as_array) {
        for track in array {
            let node = track
                .get("node")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| DocumentError::InvalidField("track.node".into()))?
                .to_string();
            let property_name = track
                .get("property")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| DocumentError::InvalidField("track.property".into()))?;
            let property = NodeProperty::from_name(property_name)
                .ok_or_else(|| DocumentError::Unsupported(format!("property {property_name}")))?;
            let mut keys = Vec::new();
            if let Some(key_array) = track.get("keys").and_then(JsonValue::as_array) {
                for key in key_array {
                    let pair = key
                        .as_array()
                        .ok_or_else(|| DocumentError::InvalidField("track.keys".into()))?;
                    if pair.len() < 2 {
                        return Err(DocumentError::InvalidField("track.keys".into()));
                    }
                    let time = pair[0].as_f32().unwrap_or(0.0);
                    let value = pair[1].as_f32().unwrap_or(0.0);
                    let easing = match pair.get(2).and_then(JsonValue::as_str) {
                        Some("ease_in") => Easing::EaseIn,
                        Some("ease_out") => Easing::EaseOut,
                        Some("ease_in_out") => Easing::EaseInOut,
                        Some("step") => Easing::Step,
                        Some("linear") | None => Easing::Linear,
                        Some(other) => {
                            return Err(DocumentError::Unsupported(format!("easing {other}")))
                        }
                    };
                    keys.push(Keyframe::new(time, value).with_easing(easing));
                }
            }
            tracks.push(TrackSpec { node, property, keys });
        }
    }
    Ok(AnimationSpec {
        name,
        duration,
        looping,
        tracks,
    })
}

/// Renders a document's nodes into a display list without instantiating assets.
///
/// Tools use this to inspect structure without decoding image data.
pub fn collect_names(document: &Document) -> Vec<String> {
    fn walk(specs: &[NodeSpec], names: &mut Vec<String>) {
        for spec in specs {
            names.push(spec.name.clone());
            walk(&spec.children, names);
        }
    }
    let mut names = Vec::new();
    walk(&document.nodes, &mut names);
    names
}

/// Convenience: transform used when a document omits position and rotation.
pub const DOCUMENT_IDENTITY: Affine2 = Affine2::IDENTITY;

/// Convenience: opaque background used by visual-novel documents.
pub const DEFAULT_BACKGROUND: Rgba8 = Rgba8::new(16, 16, 24, 255);

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"{
      "version": 1,
      "viewport": [320, 180],
      "background": "#101018",
      "nodes": [
        { "type": "group", "name": "ui", "position": [10, 20], "children": [
          { "type": "shape", "name": "panel", "shape": "round_rect",
            "rect": [0, 0, 100, 40], "radius": 6,
            "fill": "#ff8800cc", "stroke": "#ffffff", "stroke_width": 2 }
        ]},
        { "type": "text", "name": "label", "font": "body", "text": "Hi",
          "size": 12, "position": [4, 8] }
      ],
      "assets": { "fonts": { "body": "font.ttf" } }
    }"##;

    #[test]
    fn parses_structure_and_assets() {
        let document = Document::parse(SAMPLE).expect("document parses");
        assert_eq!(document.viewport, Vec2::new(320.0, 180.0));
        assert_eq!(document.background, Some(Rgba8::new(16, 16, 24, 255)));
        assert_eq!(document.assets.len(), 1);
        assert_eq!(document.assets[0].kind, AssetKind::Font);
        assert_eq!(collect_names(&document), vec!["ui", "panel", "label"]);
    }

    #[test]
    fn unknown_texture_is_rejected() {
        let text = r#"{"viewport":[1,1],"nodes":[{"type":"sprite","texture":"missing"}]}"#;
        assert!(Document::parse(text).is_err());
    }

    #[test]
    fn missing_asset_bytes_are_reported() {
        let document = Document::parse(SAMPLE).expect("document parses");
        let error = document
            .instantiate(&HashMap::new())
            .expect_err("font bytes are required");
        assert_eq!(error, DocumentError::MissingAsset("font.ttf".to_string()));
    }

    #[test]
    fn unsupported_node_types_are_rejected() {
        let text = r#"{"viewport":[1,1],"nodes":[{"type":"particles"}]}"#;
        assert!(matches!(
            Document::parse(text),
            Err(DocumentError::Unsupported(_))
        ));
    }

    #[test]
    fn gradient_paints_parse() {
        let text = r##"{"viewport":[1,1],"nodes":[{"type":"shape","shape":"rect",
            "rect":[0,0,10,10],
            "fill":{"linear":{"from":[0,0],"to":[10,0],
                    "stops":[[0,"#000000"],[1,"#ffffff"]]}}}]}"##;
        let document = Document::parse(text).expect("document parses");
        match &document.nodes[0].kind {
            NodeSpecKind::Shape(shape) => {
                assert!(matches!(shape.fill, Some(Paint::Linear(_))));
            }
            _ => panic!("expected a shape"),
        }
    }

    #[test]
    fn animations_reference_nodes_by_name() {
        let text = r#"{"viewport":[1,1],
            "nodes":[{"type":"group","name":"hero"}],
            "animations":[{"name":"idle","duration":2,"loop":true,
              "tracks":[{"node":"hero","property":"y","keys":[[0,0],[1,10,"ease_out"]]}]}]}"#;
        let document = Document::parse(text).expect("document parses");
        assert_eq!(document.animations.len(), 1);
        assert_eq!(document.animations[0].tracks[0].keys.len(), 2);
        let instantiated = document.instantiate(&HashMap::new()).expect("no assets needed");
        assert_eq!(instantiated.animations.len(), 1);
        assert_eq!(instantiated.scene.node_count(), 2);
    }
}
