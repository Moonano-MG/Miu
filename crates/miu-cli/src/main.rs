//! `miu` command-line tool.
//!
//! The binary has three jobs:
//!
//! * `plan` explains which graphics API Miu would pick for a device, using the same
//!   policy code the runtime uses. Useful when a player reports a missing backend.
//! * `info` summarises a scene document without rendering it.
//! * `render` decodes a document and writes a PNG frame, which makes the CPU
//!   renderer usable from asset pipelines and continuous integration.
//!
//! Every command is deterministic and never touches the network.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use miu_core::{
    ABI_VERSION, Backend, DeviceCapabilities, ENGINE_NAME, ENGINE_VERSION, Platform, QualityPreset,
    RenderTier, RendererRequest, plan_warning, select_best_effort, select_renderer,
};
use miu_scene::{Document, Player};

/// Exit code for usage mistakes.
const EXIT_USAGE: u8 = 2;
/// Exit code for runtime failures such as unreadable files.
const EXIT_FAILURE: u8 = 1;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Usage(message)) => {
            eprintln!("miu: {message}");
            eprintln!("{}", usage());
            ExitCode::from(EXIT_USAGE)
        }
        Err(Failure::Runtime(error)) => {
            eprintln!("miu: {error}");
            ExitCode::from(EXIT_FAILURE)
        }
    }
}

/// Two failure classes, because they deserve different exit codes.
enum Failure {
    /// The command line was wrong; the caller prints usage afterwards.
    Usage(String),
    /// The command was understood but could not complete.
    Runtime(String),
}

impl Failure {
    fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime(message.into())
    }
}

fn run(arguments: &[String]) -> Result<(), Failure> {
    let Some(command) = arguments.first() else {
        return Err(Failure::usage("no command given"));
    };
    let rest = &arguments[1..];
    match command.as_str() {
        "version" | "--version" | "-V" => {
            println!("{ENGINE_NAME} {ENGINE_VERSION} (ABI {ABI_VERSION})");
            Ok(())
        }
        "help" | "--help" | "-h" => {
            println!("{}", usage());
            Ok(())
        }
        "plan" => command_plan(rest),
        "info" => command_info(rest),
        "render" => command_render(rest),
        other => Err(Failure::usage(format!("unknown command '{other}'"))),
    }
}

fn usage() -> &'static str {
    "usage:
  miu version
  miu plan [--platform NAME] [--backends LIST] [--min TIER] [--prefer TIER]
           [--preset NAME] [--best-effort]
  miu info <document.miu.json> [--assets DIR]
  miu render <document.miu.json> --out FILE [--assets DIR] [--time SECONDS]
             [--width PIXELS] [--height PIXELS] [--preset NAME]

platforms: windows, android, macos, ios, linux, web
tiers:     compatibility, balanced, premium
presets:   low, medium, high, unlimited"
}

/// Reads the value that follows a flag, rejecting a missing value.
fn flag_value<'a>(arguments: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str, Failure> {
    *index += 1;
    arguments
        .get(*index)
        .map(String::as_str)
        .ok_or_else(|| Failure::usage(format!("{flag} needs a value")))
}

fn parse_platform(value: &str) -> Option<Platform> {
    Some(match value {
        "windows" => Platform::Windows,
        "android" => Platform::Android,
        "macos" => Platform::MacOS,
        "ios" => Platform::IOS,
        "linux" => Platform::Linux,
        "web" => Platform::Web,
        _ => return None,
    })
}

fn parse_tier(value: &str) -> Option<RenderTier> {
    Some(match value {
        "compatibility" => RenderTier::Compatibility,
        "balanced" => RenderTier::Balanced,
        "premium" => RenderTier::Premium,
        _ => return None,
    })
}

fn parse_preset(value: &str) -> Option<QualityPreset> {
    Some(match value {
        "low" => QualityPreset::Low,
        "medium" => QualityPreset::Medium,
        "high" => QualityPreset::High,
        "unlimited" => QualityPreset::Unlimited,
        _ => return None,
    })
}

fn parse_backends(list: &str) -> Result<Vec<Backend>, Failure> {
    let mut backends = Vec::new();
    for piece in list.split(',').map(str::trim).filter(|piece| !piece.is_empty()) {
        let backend = Backend::from_id(piece)
            .ok_or_else(|| Failure::usage(format!("unknown backend '{piece}'")))?;
        backends.push(backend);
    }
    if backends.is_empty() {
        return Err(Failure::usage("--backends needs at least one entry"));
    }
    Ok(backends)
}

fn command_plan(arguments: &[String]) -> Result<(), Failure> {
    let mut platform: Option<Platform> = None;
    let mut backends: Option<Vec<Backend>> = None;
    let mut minimum = RenderTier::Compatibility;
    let mut preferred = RenderTier::Premium;
    let mut preset: Option<QualityPreset> = None;
    let mut best_effort = false;

    let mut index = 0;
    while index < arguments.len() {
        let argument = arguments[index].as_str();
        match argument {
            "--platform" => {
                let value = flag_value(arguments, &mut index, "--platform")?;
                platform = Some(
                    parse_platform(value)
                        .ok_or_else(|| Failure::usage(format!("unknown platform '{value}'")))?,
                );
            }
            "--backends" => {
                let value = flag_value(arguments, &mut index, "--backends")?;
                backends = Some(parse_backends(value)?);
            }
            "--min" => {
                let value = flag_value(arguments, &mut index, "--min")?;
                minimum = parse_tier(value)
                    .ok_or_else(|| Failure::usage(format!("unknown tier '{value}'")))?;
            }
            "--prefer" => {
                let value = flag_value(arguments, &mut index, "--prefer")?;
                preferred = parse_tier(value)
                    .ok_or_else(|| Failure::usage(format!("unknown tier '{value}'")))?;
            }
            "--preset" => {
                let value = flag_value(arguments, &mut index, "--preset")?;
                preset =
                    Some(parse_preset(value).ok_or_else(|| {
                        Failure::usage(format!("unknown preset '{value}'"))
                    })?);
            }
            "--best-effort" => best_effort = true,
            other => return Err(Failure::usage(format!("unexpected argument '{other}'"))),
        }
        index += 1;
    }

    let platform = platform.unwrap_or_else(miu_core::host_platform);
    let request = RendererRequest {
        minimum_tier: minimum,
        preferred_tier: preferred,
        preset: preset.unwrap_or_else(|| preferred.max_preset()),
    };
    request
        .validate()
        .map_err(|error| Failure::usage(error.message()))?;

    let mut device = DeviceCapabilities::for_platform(platform);
    if let Some(backends) = backends {
        for backend in backends {
            device.add_backend(backend);
        }
    }

    let selection = if best_effort {
        select_best_effort(&device, &request)
    } else {
        select_renderer(&device, &request)
    };
    let plan = selection.map_err(|error| Failure::runtime(error.describe()))?;

    println!("platform       {}", device.platform.id());
    println!("requested      min={} preferred={}", minimum.id(), preferred.id());
    println!("backend        {}", plan.backend.display_name());
    println!("profile        {} ({})", plan.profile_backend.id(), plan.tier.id());
    println!("preset         {}", plan.preset.id());
    println!("emergency      {}", yes_no(plan.emergency_fallback));
    println!("below minimum  {}", yes_no(plan.below_minimum));
    let settings = plan.quality();
    println!(
        "quality        msaa={} bilinear={} dither={} sprites/frame={} glyph cache={} KiB",
        settings.msaa_samples,
        yes_no(settings.bilinear_sampling),
        yes_no(settings.gradient_dithering),
        settings.max_sprites_per_frame,
        settings.glyph_cache_bytes / 1024
    );
    if let Some(warning) = plan_warning(&plan) {
        println!("warning        {warning}");
    }
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn command_info(arguments: &[String]) -> Result<(), Failure> {
    let mut document_path: Option<PathBuf> = None;
    let mut asset_root: Option<PathBuf> = None;
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--assets" => {
                let value = flag_value(arguments, &mut index, "--assets")?;
                asset_root = Some(PathBuf::from(value));
            }
            other if other.starts_with('-') => {
                return Err(Failure::usage(format!("unexpected argument '{other}'")));
            }
            other => {
                if document_path.is_some() {
                    return Err(Failure::usage("info accepts a single document"));
                }
                document_path = Some(PathBuf::from(other));
            }
        }
        index += 1;
    }
    let document_path = document_path.ok_or_else(|| Failure::usage("info needs a document"))?;
    let root = asset_root.unwrap_or_else(|| {
        document_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let document = load_document(&document_path)?;

    println!("{ENGINE_NAME} {ENGINE_VERSION} (ABI {ABI_VERSION})");
    println!("document       {}", document_path.display());
    println!(
        "viewport       {:.0} x {:.0}",
        document.viewport.x, document.viewport.y
    );
    println!(
        "camera         position=({:.1}, {:.1}) zoom={:.3} rotation={:.1}deg",
        document.camera_position.x,
        document.camera_position.y,
        document.camera_zoom,
        document.camera_rotation.to_degrees()
    );
    println!("nodes          {}", document.nodes.len());
    println!("animations     {}", document.animations.len());
    for animation in &document.animations {
        println!(
            "  {:<16} duration={:.2}s loop={} tracks={}",
            animation.name,
            animation.duration,
            yes_no(animation.looping),
            animation.tracks.len()
        );
    }
    println!("assets         {}", document.assets.len());
    let mut missing = 0usize;
    for asset in &document.assets {
        let path = root.join(&asset.path);
        let state = if path.is_file() {
            match fs::metadata(&path) {
                Ok(metadata) => format!("{} bytes", metadata.len()),
                Err(error) => format!("unreadable ({error})"),
            }
        } else {
            missing += 1;
            "missing".to_string()
        };
        println!(
            "  {:<16} {:<8} {:<24} {state}",
            asset.name,
            match asset.kind {
                miu_scene::AssetKind::Texture => "texture",
                miu_scene::AssetKind::Font => "font",
            },
            asset.path
        );
    }
    if missing > 0 {
        println!("warning        {missing} declared asset(s) not found under {}", root.display());
    }
    Ok(())
}

fn command_render(arguments: &[String]) -> Result<(), Failure> {
    let mut document_path: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut asset_root: Option<PathBuf> = None;
    let mut time = 0.0f32;
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut preset = QualityPreset::High;

    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--out" => {
                let value = flag_value(arguments, &mut index, "--out")?;
                output = Some(PathBuf::from(value));
            }
            "--assets" => {
                let value = flag_value(arguments, &mut index, "--assets")?;
                asset_root = Some(PathBuf::from(value));
            }
            "--time" => {
                let value = flag_value(arguments, &mut index, "--time")?;
                time = value
                    .parse::<f32>()
                    .map_err(|_| Failure::usage(format!("'{value}' is not a number")))?;
            }
            "--width" => {
                let value = flag_value(arguments, &mut index, "--width")?;
                width = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| Failure::usage(format!("'{value}' is not a pixel count")))?,
                );
            }
            "--height" => {
                let value = flag_value(arguments, &mut index, "--height")?;
                height = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| Failure::usage(format!("'{value}' is not a pixel count")))?,
                );
            }
            "--preset" => {
                let value = flag_value(arguments, &mut index, "--preset")?;
                preset = parse_preset(value)
                    .ok_or_else(|| Failure::usage(format!("unknown preset '{value}'")))?;
            }
            other if other.starts_with('-') => {
                return Err(Failure::usage(format!("unexpected argument '{other}'")));
            }
            other => {
                if document_path.is_some() {
                    return Err(Failure::usage("render accepts a single document"));
                }
                document_path = Some(PathBuf::from(other));
            }
        }
        index += 1;
    }

    let document_path = document_path.ok_or_else(|| Failure::usage("render needs a document"))?;
    let output = output.ok_or_else(|| Failure::usage("render needs --out"))?;
    let root = asset_root.unwrap_or_else(|| {
        document_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    if !time.is_finite() || time < 0.0 {
        return Err(Failure::usage("--time must be a non-negative number"));
    }

    let document = load_document(&document_path)?;
    let files = read_assets(&document, &root)?;
    let mut player = Player::from_document(&document, &files)
        .map_err(|error| Failure::runtime(error.to_string()))?;
    player.set_preset(preset);
    player
        .seek(time)
        .map_err(|error| Failure::runtime(error.to_string()))?;

    let encoded = match (width, height) {
        (Some(width), Some(height)) => player.render_at_size(width, height),
        (None, None) => player.render_frame(),
        _ => {
            return Err(Failure::usage(
                "--width and --height must be given together",
            ))
        }
    }
    .map_err(|error| Failure::runtime(error.to_string()))?;

    fs::write(&output, &encoded)
        .map_err(|error| Failure::runtime(format!("cannot write {}: {error}", output.display())))?;
    let (frame_width, frame_height) = player.size();
    println!(
        "rendered {}x{} at t={time:.3}s -> {} ({} bytes)",
        frame_width,
        frame_height,
        output.display(),
        encoded.len()
    );
    Ok(())
}

fn load_document(path: &Path) -> Result<Document, Failure> {
    let text = fs::read_to_string(path)
        .map_err(|error| Failure::runtime(format!("cannot read {}: {error}", path.display())))?;
    Document::parse(&text)
        .map_err(|error| Failure::runtime(format!("{}: {error}", path.display())))
}

/// Reads every declared asset so the player can fail early with a clear message.
fn read_assets(document: &Document, root: &Path) -> Result<HashMap<String, Vec<u8>>, Failure> {
    let mut files = HashMap::new();
    for asset in &document.assets {
        let path = root.join(&asset.path);
        let bytes = fs::read(&path).map_err(|error| {
            Failure::runtime(format!("cannot read asset {}: {error}", path.display()))
        })?;
        files.insert(asset.path.clone(), bytes);
    }
    Ok(files)
}
