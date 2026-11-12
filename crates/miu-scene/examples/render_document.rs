//! Renders a scene document to PNG with the [`Player`] driver.
//!
//! Run with:
//!
//! ```text
//! cargo run -p miu-scene --example render_document -- samples/scenes/visual_novel.miu.json frame.png 1.5
//! ```
//!
//! Arguments: `<document> [output.png] [time-seconds] [width] [height]`. Asset paths
//! are resolved relative to the document, matching what an editor does when it saves a
//! scene next to its artwork.

use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs;
use std::path::Path;

use miu_scene::{Document, Player};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let document_path = arguments
        .first()
        .cloned()
        .unwrap_or_else(|| "samples/scenes/visual_novel.miu.json".to_string());
    let output_path = arguments
        .get(1)
        .cloned()
        .unwrap_or_else(|| "miu-document.png".to_string());
    let time: f32 = arguments.get(2).map(|value| value.parse()).transpose()?.unwrap_or(0.0);
    let width: Option<u32> = arguments.get(3).map(|value| value.parse()).transpose()?;
    let height: Option<u32> = arguments.get(4).map(|value| value.parse()).transpose()?;

    let path = Path::new(&document_path);
    let text = fs::read_to_string(path)?;
    let document = Document::parse(&text)?;
    let asset_root = path.parent().unwrap_or_else(|| Path::new("."));

    let mut files = HashMap::new();
    for asset in &document.assets {
        let asset_path = asset_root.join(&asset.path);
        let bytes = fs::read(&asset_path)
            .map_err(|error| format!("cannot read {}: {error}", asset_path.display()))?;
        files.insert(asset.path.clone(), bytes);
    }

    let mut player = Player::from_document(&document, &files)?;
    player.seek(time)?;
    let encoded = match (width, height) {
        (Some(width), Some(height)) => player.render_at_size(width, height)?,
        _ => player.render_frame()?,
    };
    fs::write(&output_path, &encoded)?;

    let (frame_width, frame_height) = player.size();
    println!(
        "{}: {}x{} at t={time:.2}s -> {} ({} bytes)",
        document_path,
        frame_width,
        frame_height,
        output_path,
        encoded.len()
    );
    Ok(())
}
