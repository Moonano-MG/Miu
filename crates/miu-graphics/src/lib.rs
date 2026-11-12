//! Drawing primitives for Miu: textures, paths, paints, fonts, and the CPU renderer.
//!
//! The crate is deliberately independent of any window or GPU API. It provides:
//!
//! * [`texture`] — premultiplied RGBA textures and their stores.
//! * [`batch`] — sprite instances and the sorted batch contract used by GPU bridges.
//! * [`path`] — vector outlines with curve flattening and stroking.
//! * [`paint`] — solid colours, gradients, and stroke styles.
//! * [`raster`] — an analytic anti-aliasing scanline rasteriser.
//! * [`canvas`] — a CPU render target that composites sprites, paths, and text.
//! * [`font`] / [`text`] — TrueType parsing, text layout, and glyph caching.
//! * [`codec`] — PNG (decode and encode), BMP, TGA, and QOI image codecs.
//! * [`assets`] — the texture and font stores shared by scenes and renderers.
//! * [`software`] — the reference renderer that consumes a [`display::DisplayList`].

#![deny(unsafe_code)]
#![forbid(unsafe_op_in_unsafe_fn)]

pub mod assets;
pub mod batch;
pub mod canvas;
pub mod codec;
pub mod display;
pub mod font;
pub mod paint;
pub mod path;
pub mod raster;
pub mod software;
pub mod text;
pub mod texture;

pub use assets::Assets;
pub use batch::{
    BatchBuilder, BatchMode, DrawBatch, FrameBatches, SourceRect, Sprite, UvRect,
};
pub use canvas::{Canvas, CanvasError};
pub use codec::{ImageError, decode_image, encode_png};
pub use display::{DisplayList, DrawOp, FillOp, StrokeOp, TextRun};
pub use font::{Font, FontError};
pub use paint::{
    GradientStop, LineCap, LineJoin, LinearGradient, Paint, PaintError, RadialGradient, Spread,
    StrokeStyle,
};
pub use path::{FillRule, Path, PathBuilder};
pub use raster::{CoverageMask, RasterOptions, rasterize};
pub use software::{RenderError, RenderStats, SoftwareRenderer};
pub use text::{
    FontStore, GlyphCache, PlacedGlyph, TextAlign, TextLayout, TextStyle, TextWrap, layout_text,
    measure_text,
};
pub use texture::{Texture, TextureError, TextureStore};
