//! Generates a small deterministic 2D scene using Miu's software renderer.
//!
//! Run with `cargo run -p miu-core --example software_scene`. The resulting
//! `miu-software-scene.ppm` is a portable binary image that requires no window or
//! GPU driver to inspect.

use std::fs;

use miu_core::{
    BatchBuilder, BatchMode, Camera2D, Canvas, Color, Rgba8, Sprite, Texture, TextureStore,
    SourceRect, Transform2D, UvRect, Vec2,
};

fn sprite(texture: u64, position: Vec2, scale: Vec2, layer: i32, color: Color) -> Sprite {
    Sprite {
        texture,
        transform: Transform2D {
            translation: position,
            rotation_radians: 0.0,
            scale,
        },
        source: SourceRect::FULL,
        uv: UvRect::FULL,
        size: Vec2::ZERO,
        pivot: Vec2::new(0.5, 0.5),
        color,
        layer,
    }
}

fn main() {
    const WIDTH: u32 = 320;
    const HEIGHT: u32 = 180;

    let mut textures = TextureStore::new();
    textures.insert(
        1,
        Texture::solid(
            1,
            1,
            Rgba8 {
                r: 17,
                g: 27,
                b: 54,
                a: 255,
            },
        )
        .expect("valid background texture"),
    );
    textures.insert(
        2,
        Texture::solid(
            48,
            64,
            Rgba8 {
                r: 224,
                g: 132,
                b: 96,
                a: 255,
            },
        )
        .expect("valid character texture"),
    );

    let mut commands = BatchBuilder::new(BatchMode::Stable);
    commands.push(sprite(
        1,
        Vec2::ZERO,
        Vec2::new(WIDTH as f32, HEIGHT as f32),
        -10,
        Color::WHITE,
    ));
    commands.push(sprite(
        2,
        Vec2::new(-60.0, 12.0),
        Vec2::new(1.0, 1.0),
        0,
        Color::WHITE,
    ));
    commands.push(sprite(
        2,
        Vec2::new(60.0, 12.0),
        Vec2::new(-1.0, 1.0),
        0,
        Color {
            r: 0.7,
            g: 0.85,
            b: 1.0,
            a: 1.0,
        },
    ));

    let camera = Camera2D::new(Vec2::new(WIDTH as f32, HEIGHT as f32)).expect("valid camera");
    let mut canvas = Canvas::new(WIDTH, HEIGHT).expect("valid canvas");
    canvas
        .draw_frame(&camera, &commands.finish(), &textures)
        .expect("all texture IDs are registered");

    // PPM P6 is intentionally dependency-free. Every scene colour is opaque.
    let mut ppm = format!("P6\n{} {}\n255\n", WIDTH, HEIGHT).into_bytes();
    for pixel in canvas.pixels() {
        ppm.extend_from_slice(&[pixel.r, pixel.g, pixel.b]);
    }
    fs::write("miu-software-scene.ppm", ppm).expect("write scene image");
}
