//! Scene nodes: the authored objects a game or visual-novel scene is made of.
//!
//! A node pairs a transform with one of three visual kinds — sprite, text, or shape —
//! plus grouping, layering, opacity, tags, and a name for editor and scripting use.
//! Nodes are stored in an arena owned by [`crate::Scene`], so references are plain
//! [`NodeId`] values and no reference counting is needed.

use miu_core::{BlendMode, Color, Rect, Transform2D, Vec2};
use miu_graphics::{
    Assets, Paint, Path, SourceRect, StrokeStyle, TextStyle, UvRect, layout_text,
};

/// Identifier of a node inside one scene.
pub type NodeId = u32;

/// Identifier of the scene root.
pub const ROOT: NodeId = 0;

/// Sprite appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteVisual {
    /// Texture asset id.
    pub texture: u64,
    /// Atlas source rectangle.
    pub source: SourceRect,
    /// Normalised sampling rectangle.
    pub uv: UvRect,
    /// Destination size in world units; zero means "use the source size".
    pub size: Vec2,
    /// Normalised origin inside the destination rectangle.
    pub pivot: Vec2,
    /// Premultiplied tint.
    pub color: Color,
    /// Blend mode.
    pub blend: BlendMode,
}

impl SpriteVisual {
    /// Creates a sprite using the whole texture at its natural size.
    pub fn new(texture: u64) -> Self {
        Self {
            texture,
            source: SourceRect::FULL,
            uv: UvRect::FULL,
            size: Vec2::ZERO,
            pivot: Vec2::ZERO,
            color: Color::WHITE,
            blend: BlendMode::SourceOver,
        }
    }

    /// Creates a sprite with an explicit destination size.
    pub fn with_size(texture: u64, size: Vec2) -> Self {
        let mut visual = Self::new(texture);
        visual.size = size;
        visual
    }

    /// Sets the atlas source rectangle.
    #[must_use]
    pub fn with_source(mut self, source: SourceRect) -> Self {
        self.source = source;
        self
    }

    /// Sets the pivot.
    #[must_use]
    pub fn with_pivot(mut self, pivot: Vec2) -> Self {
        self.pivot = pivot;
        self
    }

    /// Sets the tint.
    #[must_use]
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the blend mode.
    #[must_use]
    pub fn with_blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }
}

/// Text appearance and content.
#[derive(Debug, Clone, PartialEq)]
pub struct TextVisual {
    /// Font asset id.
    pub font: u64,
    /// UTF-8 text; newlines start new lines.
    pub text: String,
    /// Layout parameters.
    pub style: TextStyle,
    /// Premultiplied colour.
    pub color: Color,
    /// Blend mode.
    pub blend: BlendMode,
    /// Wrap width in world units; `None` disables wrapping.
    pub max_width: Option<f32>,
    /// Normalised origin inside the laid-out text block.
    pub pivot: Vec2,
}

impl TextVisual {
    /// Creates text at the given em size in pixels.
    pub fn new(font: u64, text: impl Into<String>, font_size: f32) -> Self {
        Self {
            font,
            text: text.into(),
            style: TextStyle::new(font_size),
            color: Color::WHITE,
            blend: BlendMode::SourceOver,
            max_width: None,
            pivot: Vec2::ZERO,
        }
    }

    /// Sets the text colour.
    #[must_use]
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Sets the layout style.
    #[must_use]
    pub fn with_style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the wrap width.
    #[must_use]
    pub fn with_max_width(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width);
        self
    }

    /// Sets the pivot.
    #[must_use]
    pub fn with_pivot(mut self, pivot: Vec2) -> Self {
        self.pivot = pivot;
        self
    }

    /// Sets the blend mode.
    #[must_use]
    pub fn with_blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }

    /// Lays the text out with its font, if the font is loaded.
    pub fn layout(&self, assets: &Assets) -> Option<miu_graphics::TextLayout> {
        let font = assets.font(self.font)?;
        Some(layout_text(font, &self.text, &self.style, self.max_width))
    }
}

/// Vector shape appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeVisual {
    /// Outline in local coordinates.
    pub path: Path,
    /// Fill paint; use a fully transparent colour to skip filling.
    pub fill: Option<Paint>,
    /// Optional stroke.
    pub stroke: Option<(StrokeStyle, Paint)>,
    /// Blend mode applied to both fill and stroke.
    pub blend: BlendMode,
    /// Normalised origin inside the shape's bounds.
    pub pivot: Vec2,
}

impl ShapeVisual {
    /// Creates a filled shape.
    pub fn new(path: Path, paint: Paint) -> Self {
        Self {
            path,
            fill: Some(paint),
            stroke: None,
            blend: BlendMode::SourceOver,
            pivot: Vec2::ZERO,
        }
    }

    /// Creates a shape that is only stroked.
    pub fn stroked(path: Path, style: StrokeStyle, paint: Paint) -> Self {
        Self {
            path,
            fill: None,
            stroke: Some((style, paint)),
            blend: BlendMode::SourceOver,
            pivot: Vec2::ZERO,
        }
    }

    /// Sets the pivot.
    #[must_use]
    pub fn with_pivot(mut self, pivot: Vec2) -> Self {
        self.pivot = pivot;
        self
    }

    /// Sets the blend mode.
    #[must_use]
    pub fn with_blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }

    /// Sets the fill paint, or removes the fill.
    #[must_use]
    pub fn with_fill(mut self, fill: Option<Paint>) -> Self {
        self.fill = fill;
        self
    }

    /// Sets the stroke.
    #[must_use]
    pub fn with_stroke(mut self, stroke: Option<(StrokeStyle, Paint)>) -> Self {
        self.stroke = stroke;
        self
    }
}

/// What a node draws.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Draws nothing itself; exists to transform and group children.
    Group,
    /// Textured quad.
    Sprite(SpriteVisual),
    /// Laid-out text.
    Text(TextVisual),
    /// Vector shape.
    Shape(ShapeVisual),
}

impl NodeKind {
    /// Short type name used by documents and tools.
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Sprite(_) => "sprite",
            Self::Text(_) => "text",
            Self::Shape(_) => "shape",
        }
    }
}

/// One object in a scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Editor-facing name, unique only if the author keeps it so.
    pub name: String,
    /// What the node draws.
    pub kind: NodeKind,
    /// Local transform relative to the parent.
    pub transform: Transform2D,
    /// Parent node, if any; the root has none.
    pub parent: Option<NodeId>,
    /// Child nodes in submission order.
    pub children: Vec<NodeId>,
    /// Whether the node and its subtree are drawn.
    pub visible: bool,
    /// Opacity multiplied into the subtree, in `[0, 1]`.
    pub opacity: f32,
    /// Painter layer; lower layers draw first.
    pub layer: i32,
    /// Free-form tags for editor filters and scripting.
    pub tags: Vec<String>,
}

impl Node {
    /// Creates a node with a visual kind.
    pub fn new(name: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            name: name.into(),
            kind,
            transform: Transform2D::IDENTITY,
            parent: None,
            children: Vec::new(),
            visible: true,
            opacity: 1.0,
            layer: 0,
            tags: Vec::new(),
        }
    }

    /// Creates an empty group.
    pub fn group(name: impl Into<String>) -> Self {
        Self::new(name, NodeKind::Group)
    }

    /// Creates a sprite node.
    pub fn sprite(name: impl Into<String>, visual: SpriteVisual) -> Self {
        Self::new(name, NodeKind::Sprite(visual))
    }

    /// Creates a text node.
    pub fn text(name: impl Into<String>, visual: TextVisual) -> Self {
        Self::new(name, NodeKind::Text(visual))
    }

    /// Creates a shape node.
    pub fn shape(name: impl Into<String>, visual: ShapeVisual) -> Self {
        Self::new(name, NodeKind::Shape(visual))
    }

    /// Sets the local position.
    #[must_use]
    pub fn with_position(mut self, position: Vec2) -> Self {
        self.transform.translation = position;
        self
    }

    /// Sets the local transform.
    #[must_use]
    pub fn with_transform(mut self, transform: Transform2D) -> Self {
        self.transform = transform;
        self
    }

    /// Sets the painter layer.
    #[must_use]
    pub fn with_layer(mut self, layer: i32) -> Self {
        self.layer = layer;
        self
    }

    /// Sets the opacity.
    #[must_use]
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 0.0 };
        self
    }

    /// Sets visibility.
    #[must_use]
    pub fn with_visibility(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Adds a tag.
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Whether the node can contribute pixels at all.
    pub fn is_drawable(&self) -> bool {
        self.visible && self.opacity > 0.0 && !matches!(self.kind, NodeKind::Group)
    }

    /// Whether the node carries a visual payload.
    pub fn has_visual(&self) -> bool {
        !matches!(self.kind, NodeKind::Group)
    }

    /// Local-space bounds before the node transform, if the kind has geometry.
    pub fn local_bounds(&self, assets: &Assets) -> Option<Rect> {
        match &self.kind {
            NodeKind::Group => None,
            NodeKind::Sprite(sprite) => {
                let size = if sprite.size.x > 0.0 && sprite.size.y > 0.0 {
                    sprite.size
                } else {
                    let texture = assets.texture(sprite.texture)?;
                    let source = if sprite.source.is_full_sentinel() {
                        Vec2::new(texture.width() as f32, texture.height() as f32)
                    } else {
                        Vec2::new(sprite.source.width as f32, sprite.source.height as f32)
                    };
                    source
                };
                let origin = Vec2::new(-size.x * sprite.pivot.x, -size.y * sprite.pivot.y);
                Some(Rect::from_min_max(origin, origin + size))
            }
            NodeKind::Text(text) => {
                let layout = text.layout(assets)?;
                let origin = Vec2::new(
                    -layout.width * text.pivot.x,
                    -layout.height * text.pivot.y,
                );
                Some(Rect::from_min_max(origin, origin + layout.size()))
            }
            NodeKind::Shape(shape) => {
                let bounds = shape.path.bounds();
                // The shape is translated so its pivot lands on the node origin.
                let half = Vec2::new(
                    bounds.width * shape.pivot.x,
                    bounds.height * shape.pivot.y,
                );
                let origin = Vec2::new(-half.x, -half.y);
                Some(Rect::from_min_max(
                    origin,
                    origin + Vec2::new(bounds.width, bounds.height),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miu_core::Color;

    #[test]
    fn node_builders_keep_values() {
        let node = Node::sprite("hero", SpriteVisual::with_size(7, Vec2::new(32.0, 64.0)))
            .with_position(Vec2::new(10.0, 20.0))
            .with_layer(3)
            .with_opacity(0.5)
            .with_tag("character");
        assert_eq!(node.name, "hero");
        assert_eq!(node.layer, 3);
        assert_eq!(node.opacity, 0.5);
        assert_eq!(node.tags, vec!["character".to_string()]);
        assert!(node.is_drawable());
        assert_eq!(node.kind.type_name(), "sprite");
    }

    #[test]
    fn groups_are_not_drawable_but_keep_children() {
        let mut group = Node::group("ui");
        group.children.push(4);
        assert!(!group.is_drawable());
        assert!(!group.has_visual());
        assert!(group.local_bounds(&Assets::new()).is_none());
    }

    #[test]
    fn text_visual_reports_type() {
        let text = TextVisual::new(1, "Hello", 24.0);
        let node = Node::text("label", text.with_color(Color::WHITE));
        assert_eq!(node.kind.type_name(), "text");
    }
}
