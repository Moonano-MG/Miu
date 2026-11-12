//! Scene graph, animation, documents, and the frame driver.
//!
//! The crate builds on [`miu_graphics`] and adds the pieces a game or visual-novel
//! editor works with:
//!
//! * [`node`] — sprites, text, shapes, and groups with transforms, layers, and tags.
//! * [`scene`] — arena storage, traversal, hit testing, and display-list building.
//! * [`animation`] — keyframes, easing curves, looping, and fade transitions.
//! * [`json`] / [`document`] — the JSON scene document format and its loader.
//! * [`player`] — playback and rendering through the software renderer.
//!
//! ```no_run
//! use std::collections::HashMap;
//! use miu_scene::{Document, Player};
//!
//! let text = std::fs::read_to_string("samples/scenes/visual_novel.miu.json")?;
//! let document = Document::parse(&text)?;
//! let files = HashMap::new();
//! let player = Player::from_document(&document, &files)?;
//! let png = player.render_frame()?;
//! std::fs::write("frame.png", png)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![deny(unsafe_code)]
#![forbid(unsafe_op_in_unsafe_fn)]

pub mod animation;
pub mod document;
pub mod json;
pub mod node;
pub mod player;
pub mod scene;

pub use animation::{Animation, Easing, Fade, Keyframe, NodeProperty, Track};
pub use document::{
    AnimationSpec, AssetKind, AssetRef, Document, DocumentError, Instantiated, NodeSpec,
    NodeSpecKind, ResolvedAsset, ShapeSpec, SpriteSpec, TextSpec, TrackSpec,
};
pub use json::{JsonError, JsonValue};
pub use node::{Node, NodeId, NodeKind, ShapeVisual, SpriteVisual, TextVisual, ROOT};
pub use player::{Player, PlayerError};
pub use scene::{Scene, SceneError};
