//! Draws a small scene with the CPU renderer and writes `miu-software-scene.png`.
//!
//! Run with `cargo run -p miu-graphics --example software_scene`. The example needs
//! no window, GPU, or asset files: every texture is generated procedurally, so it is
//! the fastest way to check that the rasteriser, gradients, and blending behave after
//! a change.

use std::fs;

use miu_core::{Affine2, BlendMode, Camera2D, Color, Paint, Rect, Rgba8, Vec2};
use miu_graphics::{
    DisplayList, DrawOp, FillOp, FillRule, GradientStop, Path, Spread, Sprite, SoftwareRenderer,
    Texture,
};

const WIDTH: u32 = 480;
const HEIGHT: u32 = 270;

/// Checkerboard with a soft diagonal gradient, premultiplied on the way in.
fn checkerboard(size: u32, cell: u32) -> Texture {
    let mut straight = Vec::with_capacity((size * size) as usize);
    for y in 0..size {
        for x in 0..size {
            let light = ((x / cell) + (y / cell)) % 2 == 0;
            let base = if light { 232 } else { 36 };
            let tint = (x * 255 / size.max(1)) as u8;
            straight.push(Rgba8::new(
                base,
                (base as u16 * 3 / 4) as u8,
                tint,
                255,
            ));
        }
    }
    Texture::from_straight_alpha(size, size, &straight).expect("checkerboard texture")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut renderer = SoftwareRenderer::new(WIDTH, HEIGHT)?;
    let camera = Camera2D::new(Vec2::new(WIDTH as f32, HEIGHT as f32))?;
    renderer
        .assets_mut()
        .insert_texture(1, checkerboard(128, 16));

    let mut list = DisplayList::new();

    // Background with a vertical gradient.
    let background = Paint::linear(
        Vec2::new(0.0, 0.0),
        Vec2::new(0.0, HEIGHT as f32),
        vec![
            GradientStop::new(0.0, Color::from_rgb8(14, 18, 32)),
            GradientStop::new(1.0, Color::from_rgb8(38, 24, 48)),
        ],
        Spread::Pad,
    )?;
    list.push(DrawOp::Fill(FillOp {
        path: Path::rect(Rect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32)),
        transform: Affine2::IDENTITY,
        paint: background,
        rule: FillRule::NonZero,
        blend: BlendMode::SourceOver,
        cull_bounds: None,
    }));

    // Three sprites on different layers, one of them additive.
    for (index, (x, y, rotation, blend)) in [
        (120.0f32, 150.0f32, 0.0f32, BlendMode::SourceOver),
        (250.0f32, 130.0f32, 0.4f32, BlendMode::Add),
        (360.0f32, 170.0f32, -0.25f32, BlendMode::SourceOver),
    ]
    .into_iter()
    .enumerate()
    {
        let mut sprite = Sprite::new(1)
            .with_trs(miu_core::Transform2D::new(
                Vec2::new(x, y),
                rotation,
                Vec2::splat(0.9),
            ))
            .with_size(Vec2::splat(96.0))
            .with_pivot(Vec2::splat(0.5))
            .with_blend(blend)
            .with_layer(index as i32);
        sprite.color = Color::from_rgb8(255, 210, 180);
        list.push(DrawOp::Sprite(sprite));
    }

    // A stroked circle on top, to exercise the stroker and the analytic mask.
    list.push(DrawOp::Stroke(miu_graphics::StrokeOp {
        path: Path::circle(Vec2::new(240.0, 135.0), 86.0),
        transform: Affine2::IDENTITY,
        style: miu_graphics::StrokeStyle::new(6.0),
        paint: Paint::solid(Color::from_rgb8(120, 230, 255)),
        blend: BlendMode::SourceOver,
        cull_bounds: None,
    }));

    renderer.begin_frame(Some(Rgba8::new(0, 0, 0, 255)))?;
    renderer.draw(&camera, &list)?;

    let stats = renderer.stats();
    fs::write("miu-software-scene.png", renderer.encode_png()?)?;
    println!(
        "miu-software-scene.png: {} sprites, {} fills, {} strokes, {} covered pixels",
        stats.sprites, stats.fills, stats.strokes, stats.covered_pixels
    );
    Ok(())
}
