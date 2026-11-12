//! Scene graph storage, traversal, hit testing, and display-list building.
//!
//! [`Scene`] owns an arena of [`Node`]s plus one camera and a viewport size. Building
//! a frame walks the tree, accumulates transforms and opacity, and emits a
//! [`DisplayList`] in painter order. Children are sorted by `layer` (ascending) and
//! then by insertion order, so layering is stable and independent of traversal order.

use miu_core::{Affine2, Camera2D, CameraError, Rect, Rgba8, Vec2};
use miu_graphics::{
    Assets, BlendMode, DisplayList, DrawOp, FillOp, Sprite, StrokeOp, TextRun,
};

use crate::node::{Node, NodeId, NodeKind, ROOT};

/// Failure while editing a scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneError {
    /// The node id is not present in this scene.
    InvalidNode(NodeId),
    /// The requested parent id is not present in this scene.
    InvalidParent(NodeId),
    /// Re-parenting would create a cycle.
    Cycle,
    /// The root node cannot be removed or re-parented.
    RootImmutable,
    /// The scene has no room for another node.
    TooManyNodes,
    /// The camera configuration was rejected.
    Camera(CameraError),
}

impl std::error::Error for SceneError {}

impl core::fmt::Display for SceneError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidNode(id) => write!(formatter, "node {id} does not exist"),
            Self::InvalidParent(id) => write!(formatter, "parent node {id} does not exist"),
            Self::Cycle => formatter.write_str("re-parenting would create a cycle"),
            Self::RootImmutable => formatter.write_str("the root node is immutable"),
            Self::TooManyNodes => formatter.write_str("scene node limit reached"),
            Self::Camera(error) => write!(formatter, "camera error: {}", error.id()),
        }
    }
}

impl From<CameraError> for SceneError {
    fn from(error: CameraError) -> Self {
        Self::Camera(error)
    }
}

/// Arena-backed 2D scene with one camera.
#[derive(Debug, Clone)]
pub struct Scene {
    nodes: Vec<Option<Node>>,
    free: Vec<NodeId>,
    root: NodeId,
    viewport: Vec2,
    background: Option<Rgba8>,
    camera: Camera2D,
    revision: u64,
}

impl Scene {
    /// Creates a scene with a root group and a camera framed to `viewport`.
    pub fn new(viewport: Vec2) -> Result<Self, SceneError> {
        let camera = Camera2D::new(viewport)?;
        let mut root = Node::group("root");
        root.layer = i32::MIN;
        Ok(Self {
            nodes: vec![Some(root)],
            free: Vec::new(),
            root: ROOT,
            viewport,
            background: None,
            camera,
            revision: 1,
        })
    }

    /// Root node id; it always exists and always groups children.
    pub const fn root(&self) -> NodeId {
        self.root
    }

    /// Viewport size in logical pixels.
    pub const fn viewport(&self) -> Vec2 {
        self.viewport
    }

    /// Resizes the viewport and the camera behind it.
    pub fn set_viewport(&mut self, viewport: Vec2) -> Result<(), SceneError> {
        self.camera.resize(viewport)?;
        self.viewport = viewport;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Camera used to render this scene.
    pub fn camera(&self) -> &Camera2D {
        &self.camera
    }

    /// Mutable camera, for animation and input handling.
    pub fn camera_mut(&mut self) -> &mut Camera2D {
        &mut self.camera
    }

    /// Optional background colour used to clear the framebuffer before drawing.
    pub fn background(&self) -> Option<Rgba8> {
        self.background
    }

    /// Sets the background clear colour.
    pub fn set_background(&mut self, background: Option<Rgba8>) {
        self.background = background;
        self.revision = self.revision.wrapping_add(1);
    }

    /// Monotonically increasing counter bumped by structural edits.
    ///
    /// Editors compare revisions to decide when to rebuild caches. In-place
    /// [`Scene::node_mut`] edits are not tracked; bump the revision by calling
    /// [`Scene::touch`] when properties change through that path.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Marks the scene as modified.
    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Looks up a node.
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id as usize).and_then(|slot| slot.as_ref())
    }

    /// Looks up a node mutably.
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id as usize).and_then(|slot| slot.as_mut())
    }

    /// Child ids of a node, or an empty slice when the node is missing.
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        match self.node(id) {
            Some(node) => node.children.as_slice(),
            None => &[],
        }
    }

    /// Number of live nodes, including the root.
    pub fn node_count(&self) -> usize {
        self.nodes.len() - self.free.len()
    }

    /// Adds a node under `parent` and returns its id.
    pub fn create(&mut self, parent: NodeId, mut node: Node) -> Result<NodeId, SceneError> {
        if self.node(parent).is_none() {
            return Err(SceneError::InvalidParent(parent));
        }
        node.parent = Some(parent);
        node.children.clear();
        let id = match self.free.pop() {
            Some(id) => {
                self.nodes[id as usize] = Some(node);
                id
            }
            None => {
                if self.nodes.len() >= u32::MAX as usize {
                    return Err(SceneError::TooManyNodes);
                }
                self.nodes.push(Some(node));
                (self.nodes.len() - 1) as NodeId
            }
        };
        if let Some(parent_node) = self.node_mut(parent) {
            parent_node.children.push(id);
        }
        self.revision = self.revision.wrapping_add(1);
        Ok(id)
    }

    /// Adds a node under the root.
    pub fn add(&mut self, node: Node) -> Result<NodeId, SceneError> {
        self.create(self.root, node)
    }

    /// Removes a node and its whole subtree.
    pub fn remove(&mut self, id: NodeId) -> Result<(), SceneError> {
        if id == self.root {
            return Err(SceneError::RootImmutable);
        }
        let node = self.node(id).ok_or(SceneError::InvalidNode(id))?;
        let parent = node.parent.ok_or(SceneError::RootImmutable)?;
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            let taken = self
                .nodes
                .get_mut(current as usize)
                .and_then(|slot| slot.take());
            if let Some(removed) = taken {
                stack.extend(removed.children.iter().copied());
                self.free.push(current);
            }
        }
        if let Some(parent_node) = self.node_mut(parent) {
            parent_node.children.retain(|child| *child != id);
        }
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Moves a node under a different parent.
    pub fn set_parent(&mut self, id: NodeId, new_parent: NodeId) -> Result<(), SceneError> {
        if id == self.root {
            return Err(SceneError::RootImmutable);
        }
        if self.node(id).is_none() {
            return Err(SceneError::InvalidNode(id));
        }
        if self.node(new_parent).is_none() {
            return Err(SceneError::InvalidParent(new_parent));
        }
        let mut cursor = Some(new_parent);
        while let Some(current) = cursor {
            if current == id {
                return Err(SceneError::Cycle);
            }
            cursor = self.node(current).and_then(|node| node.parent);
        }
        let previous = self.node(id).and_then(|node| node.parent);
        if let Some(previous) = previous {
            if let Some(node) = self.node_mut(previous) {
                node.children.retain(|child| *child != id);
            }
        }
        if let Some(node) = self.node_mut(id) {
            node.parent = Some(new_parent);
        }
        if let Some(node) = self.node_mut(new_parent) {
            node.children.push(id);
        }
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Iterates over every live `(id, node)` pair in arena order.
    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|node| (index as NodeId, node)))
    }

    /// Finds the first node with exactly this name.
    pub fn find_by_name(&self, name: &str) -> Option<NodeId> {
        self.iter()
            .find(|(_, node)| node.name == name)
            .map(|(id, _)| id)
    }

    /// Collects every node carrying `tag`.
    pub fn find_by_tag(&self, tag: &str) -> Vec<NodeId> {
        self.iter()
            .filter(|(_, node)| node.tags.iter().any(|value| value == tag))
            .map(|(id, _)| id)
            .collect()
    }

    /// Accumulated local-to-world matrix, including the node's own transform.
    pub fn world_affine(&self, id: NodeId) -> Option<Affine2> {
        let mut chain: Vec<&Node> = Vec::new();
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            let node = self.node(current)?;
            chain.push(node);
            cursor = node.parent;
        }
        let mut affine = Affine2::IDENTITY;
        for node in chain.iter().rev() {
            affine = node.transform.to_affine().then(affine);
        }
        Some(affine)
    }

    /// Product of the node's own opacity and every ancestor's opacity.
    pub fn world_opacity(&self, id: NodeId) -> f32 {
        let mut opacity = 1.0f32;
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            match self.node(current) {
                Some(node) => {
                    opacity *= node.opacity.clamp(0.0, 1.0);
                    cursor = node.parent;
                }
                None => break,
            }
        }
        opacity
    }

    /// Whether the node and all its ancestors are visible.
    pub fn is_effectively_visible(&self, id: NodeId) -> bool {
        let mut cursor = Some(id);
        while let Some(current) = cursor {
            match self.node(current) {
                Some(node) => {
                    if !node.visible || node.opacity <= 0.0 {
                        return false;
                    }
                    cursor = node.parent;
                }
                None => return false,
            }
        }
        true
    }

    /// Axis-aligned world bounds of a node's own geometry.
    pub fn world_bounds(&self, id: NodeId, assets: &Assets) -> Option<Rect> {
        let node = self.node(id)?;
        let local = node.local_bounds(assets)?;
        let affine = self.world_affine(id)?;
        Some(transform_rect(&affine, local))
    }

    /// Sets a node's local transform.
    pub fn set_transform(&mut self, id: NodeId, transform: miu_core::Transform2D) -> Result<(), SceneError> {
        let node = self.node_mut(id).ok_or(SceneError::InvalidNode(id))?;
        node.transform = transform;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Translates a node in its parent's space.
    pub fn translate(&mut self, id: NodeId, delta: Vec2) -> Result<(), SceneError> {
        let node = self.node_mut(id).ok_or(SceneError::InvalidNode(id))?;
        node.transform.translation += delta;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    /// Node ids in paint order (back to front), skipping hidden subtrees.
    pub fn draw_order(&self) -> Vec<NodeId> {
        let mut order = Vec::new();
        self.collect_order(self.root, &mut order);
        order
    }

    fn collect_order(&self, id: NodeId, order: &mut Vec<NodeId>) {
        let Some(node) = self.node(id) else {
            return;
        };
        if !node.visible || node.opacity <= 0.0 {
            return;
        }
        if node.has_visual() {
            order.push(id);
        }
        for child in self.sorted_children(node) {
            self.collect_order(child, order);
        }
    }

    /// Returns the topmost drawable node containing `world_point`.
    pub fn hit_test(&self, assets: &Assets, world_point: Vec2) -> Option<NodeId> {
        if !world_point.is_finite() {
            return None;
        }
        let order = self.draw_order();
        for id in order.iter().rev() {
            if let Some(bounds) = self.world_bounds(*id, assets) {
                if bounds.expand(0.5).contains(world_point) {
                    return Some(*id);
                }
            }
        }
        None
    }

    /// Builds a fresh display list for this frame.
    pub fn build(&self, assets: &Assets) -> DisplayList {
        let mut list = DisplayList::new();
        self.build_into(assets, &mut list);
        list
    }

    /// Appends this scene's operations to an existing display list.
    ///
    /// Text nodes whose font is not loaded are skipped; the caller can detect that
    /// with [`Scene::missing_assets`] before rendering.
    pub fn build_into(&self, assets: &Assets, list: &mut DisplayList) {
        self.emit(self.root, Affine2::IDENTITY, 1.0, assets, list);
    }

    /// Font and texture ids referenced by the scene but absent from `assets`.
    pub fn missing_assets(&self, assets: &Assets) -> (Vec<u64>, Vec<u64>) {
        let mut missing_textures = Vec::new();
        let mut missing_fonts = Vec::new();
        for (_, node) in self.iter() {
            match &node.kind {
                NodeKind::Sprite(sprite) => {
                    if assets.texture(sprite.texture).is_none()
                        && !missing_textures.contains(&sprite.texture)
                    {
                        missing_textures.push(sprite.texture);
                    }
                }
                NodeKind::Text(text) => {
                    if assets.font(text.font).is_none() && !missing_fonts.contains(&text.font) {
                        missing_fonts.push(text.font);
                    }
                }
                _ => {}
            }
        }
        (missing_textures, missing_fonts)
    }

    fn sorted_children(&self, node: &Node) -> Vec<NodeId> {
        let mut children = node.children.clone();
        children.sort_by_key(|child| {
            self.node(*child)
                .map(|node| node.layer)
                .unwrap_or(i32::MAX)
        });
        children
    }

    fn emit(
        &self,
        id: NodeId,
        parent_affine: Affine2,
        parent_opacity: f32,
        assets: &Assets,
        list: &mut DisplayList,
    ) {
        let Some(node) = self.node(id) else {
            return;
        };
        if !node.visible || node.opacity <= 0.0 || parent_opacity <= 0.0 {
            return;
        }
        let opacity = parent_opacity * node.opacity.clamp(0.0, 1.0);
        let affine = node.transform.to_affine().then(parent_affine);

        match &node.kind {
            NodeKind::Group => {
                for child in self.sorted_children(node) {
                    self.emit(child, affine, opacity, assets, list);
                }
            }
            NodeKind::Sprite(visual) => {
                let mut sprite = Sprite::new(visual.texture)
                    .with_source(visual.source)
                    .with_size(visual.size)
                    .with_pivot(visual.pivot)
                    .with_color(visual.color.multiply_alpha(opacity))
                    .with_blend(visual.blend)
                    .with_layer(node.layer)
                    .with_transform(affine);
                sprite.uv = visual.uv;
                list.push(DrawOp::Sprite(sprite));
            }
            NodeKind::Text(visual) => {
                let Some(layout) = visual.layout(assets) else {
                    return;
                };
                if layout.glyphs.is_empty() {
                    return;
                }
                let offset = Vec2::new(
                    -layout.width * visual.pivot.x,
                    -layout.height * visual.pivot.y,
                );
                let transform = Affine2::from_translation(offset).then(affine);
                let cull = transform_rect(&transform, Rect::new(0.0, 0.0, layout.width, layout.height));
                list.push(DrawOp::Text(TextRun {
                    font: visual.font,
                    size: visual.style.em_size(),
                    color: visual.color.multiply_alpha(opacity),
                    blend: visual.blend,
                    transform,
                    glyphs: layout.glyphs,
                    cull_bounds: Some(cull),
                }));
            }
            NodeKind::Shape(visual) => {
                let bounds = visual.path.bounds();
                let offset = Vec2::new(
                    -bounds.width * visual.pivot.x,
                    -bounds.height * visual.pivot.y,
                );
                let transform = Affine2::from_translation(offset).then(affine);
                let cull = transform_rect(&transform, bounds);
                if let Some(paint) = &visual.fill {
                    list.push(DrawOp::Fill(FillOp {
                        path: visual.path.clone(),
                        transform,
                        paint: paint.clone().with_opacity(opacity),
                        rule: miu_graphics::FillRule::NonZero,
                        blend: visual.blend,
                        cull_bounds: Some(cull),
                    }));
                }
                if let Some((style, paint)) = &visual.stroke {
                    list.push(DrawOp::Stroke(StrokeOp {
                        path: visual.path.clone(),
                        transform,
                        style: style.clone(),
                        paint: paint.clone().with_opacity(opacity),
                        blend: visual.blend,
                        cull_bounds: Some(cull),
                    }));
                }
            }
        }
    }
}

fn transform_rect(affine: &Affine2, rect: Rect) -> Rect {
    let corners = [
        affine.transform_point(rect.min()),
        affine.transform_point(Vec2::new(rect.max().x, rect.min().y)),
        affine.transform_point(rect.max()),
        affine.transform_point(Vec2::new(rect.min().x, rect.max().y)),
    ];
    let mut minimum = corners[0];
    let mut maximum = corners[0];
    for corner in corners.iter().skip(1) {
        minimum = minimum.min(*corner);
        maximum = maximum.max(*corner);
    }
    Rect::from_min_max(minimum, maximum)
}

/// Convenience: blend mode used by UI canvases that composite additively.
pub const ADDITIVE: BlendMode = BlendMode::Add;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{SpriteVisual, TextVisual};
    use miu_core::{Color, Transform2D};
    use miu_graphics::{Paint, Path};

    fn scene() -> Scene {
        Scene::new(Vec2::new(320.0, 180.0)).expect("viewport is valid")
    }

    #[test]
    fn create_and_remove_nodes_updates_the_arena() {
        let mut scene = scene();
        let id = scene.add(Node::group("ui")).unwrap();
        assert_eq!(scene.node_count(), 2);
        assert_eq!(scene.children(ROOT), &[id]);
        scene.remove(id).unwrap();
        assert_eq!(scene.node_count(), 1);
        assert!(scene.node(id).is_none());
        let reuse = scene.add(Node::group("ui2")).unwrap();
        assert_eq!(reuse, id, "freed slots are reused");
    }

    #[test]
    fn removing_the_root_is_rejected() {
        let mut scene = scene();
        assert_eq!(scene.remove(ROOT), Err(SceneError::RootImmutable));
        assert_eq!(scene.set_parent(ROOT, 1), Err(SceneError::RootImmutable));
    }

    #[test]
    fn reparenting_to_a_descendant_is_a_cycle() {
        let mut scene = scene();
        let parent = scene.add(Node::group("parent")).unwrap();
        let child = scene.create(parent, Node::group("child")).unwrap();
        assert_eq!(scene.set_parent(parent, child), Err(SceneError::Cycle));
    }

    #[test]
    fn world_transform_and_opacity_accumulate() {
        let mut scene = scene();
        let parent = scene
            .create(
                ROOT,
                Node::group("parent")
                    .with_position(Vec2::new(10.0, 5.0))
                    .with_transform(Transform2D::new(Vec2::new(10.0, 5.0), 0.0, Vec2::ONE))
                    .with_opacity(0.5),
            )
            .unwrap();
        let child = scene
            .create(parent, Node::group("child").with_position(Vec2::new(4.0, 1.0)))
            .unwrap();
        let affine = scene.world_affine(child).unwrap();
        assert!(affine.transform_point(Vec2::ZERO).approx_eq(Vec2::new(14.0, 6.0), 0.001));
        assert!((scene.world_opacity(child) - 0.5).abs() < 0.0001);
    }

    #[test]
    fn building_emits_children_in_layer_order() {
        let mut scene = scene();
        let mut assets = Assets::new();
        assets.insert_texture(1, miu_graphics::Texture::solid(4, 4, Rgba8::WHITE).unwrap());
        scene
            .add(Node::sprite(
                "front",
                SpriteVisual::with_size(1, Vec2::new(4.0, 4.0)),
            )
            .with_layer(10))
            .unwrap();
        scene
            .add(Node::sprite(
                "back",
                SpriteVisual::with_size(1, Vec2::new(4.0, 4.0)),
            )
            .with_layer(-10))
            .unwrap();
        let list = scene.build(&assets);
        let layers: Vec<i32> = list
            .ops()
            .iter()
            .filter_map(|op| match op {
                DrawOp::Sprite(sprite) => Some(sprite.layer),
                _ => None,
            })
            .collect();
        assert_eq!(layers, vec![-10, 10]);
    }

    #[test]
    fn hidden_subtrees_are_not_built() {
        let mut scene = scene();
        let group = scene
            .add(Node::group("hidden").with_visibility(false))
            .unwrap();
        scene
            .create(
                group,
                Node::shape(
                    "square",
                    crate::node::ShapeVisual::new(
                        Path::rect(Rect::new(0.0, 0.0, 4.0, 4.0)),
                        Paint::solid(Color::WHITE),
                    ),
                ),
            )
            .unwrap();
        let list = scene.build(&Assets::new());
        assert!(list.is_empty());
        assert!(scene.draw_order().is_empty());
    }

    #[test]
    fn text_nodes_require_a_loaded_font() {
        let mut scene = scene();
        scene
            .add(Node::text("dialogue", TextVisual::new(3, "Hello", 20.0)))
            .unwrap();
        let list = scene.build(&Assets::new());
        assert!(list.is_empty());
        let (textures, fonts) = scene.missing_assets(&Assets::new());
        assert!(textures.is_empty());
        assert_eq!(fonts, vec![3]);
    }

    #[test]
    fn hit_testing_returns_the_topmost_node() {
        let scene_bounds = Vec2::new(32.0, 32.0);
        let mut scene = Scene::new(scene_bounds).unwrap();
        let mut assets = Assets::new();
        assets.insert_texture(1, miu_graphics::Texture::solid(8, 8, Rgba8::WHITE).unwrap());
        let back = scene
            .add(
                Node::sprite("back", SpriteVisual::with_size(1, Vec2::new(8.0, 8.0)))
                    .with_layer(0),
            )
            .unwrap();
        let front = scene
            .add(
                Node::sprite("front", SpriteVisual::with_size(1, Vec2::new(8.0, 8.0)))
                    .with_layer(5),
            )
            .unwrap();
        let _ = back;
        assert_eq!(scene.hit_test(&assets, Vec2::new(4.0, 4.0)), Some(front));
        assert_eq!(scene.hit_test(&assets, Vec2::new(100.0, 100.0)), None);
    }

    #[test]
    fn find_by_name_and_tag_work() {
        let mut scene = scene();
        let id = scene
            .add(Node::group("dialogue").with_tag("ui"))
            .unwrap();
        assert_eq!(scene.find_by_name("dialogue"), Some(id));
        assert_eq!(scene.find_by_tag("ui"), vec![id]);
    }
}
