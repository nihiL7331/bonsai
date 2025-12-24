use crate::assets::generate_sprite_metadata;
use crate::error::CustomError;
use colored::Colorize;
use std::fs::{self};
use std::path::{Path, PathBuf};
use texture_packer::{TexturePacker, TexturePackerConfig, exporter::ImageExporter};
use walkdir::WalkDir;

const ATLAS_NAME: &str = "atlas.png";

struct AtlasContext {
    images_dir: PathBuf,
    atlas_path: PathBuf,
    atlas_dir: PathBuf,
    verbose: bool,
}

struct AtlasOutput {
    width: u32,
    height: u32,
}

impl AtlasContext {
    fn new(assets_dir: &Path, atlas_dir: &Path, verbose: bool) -> Self {
        let images_dir = assets_dir.join("images");
        let atlas_dir = PathBuf::from(atlas_dir);
        let atlas_path = atlas_dir.join(ATLAS_NAME);

        Self {
            images_dir,
            atlas_path,
            atlas_dir,
            verbose,
        }
    }

    fn info(&self, msg: impl AsRef<str>) {
        if self.verbose {
            println!("{} {}", "[INFO]".green(), msg.as_ref());
        }
    }
}

pub fn pack_atlas(assets_dir: &Path, atlas_dir: &Path, verbose: bool) -> Result<(), CustomError> {
    let ctx = AtlasContext::new(assets_dir, atlas_dir, verbose);

    if !should_repack(&ctx.images_dir, &ctx.atlas_path)? {
        ctx.info("Atlas is up to date. Skipping packing.");
        return Ok(());
    }

    ctx.info("Packing texture atlas...");

    let config = TexturePackerConfig {
        max_width: 2048,
        max_height: 2048,
        allow_rotation: false,
        texture_outlines: false,
        border_padding: 2,
        texture_padding: 2,
        trim: false,
        ..Default::default()
    };
    let mut packer = TexturePacker::new_skyline(config);
    collect_images(&ctx, &mut packer)?;
    let output = write_atlas(&ctx, &packer)?;
    generate_sprite_metadata(&packer, output.width, output.height)?;

    Ok(())
}

fn collect_images(
    ctx: &AtlasContext,
    packer: &mut TexturePacker<image::RgbaImage, String>,
) -> Result<(), CustomError> {
    if !ctx.images_dir.exists() {
        ctx.info("No 'images' folder found in assets. Skipping.");
        return Ok(());
    }

    for entry in WalkDir::new(&ctx.images_dir) {
        let entry = entry.map_err(|e| CustomError::IoError(e.into()))?;
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        if path.file_name().and_then(|s| s.to_str()) == Some(ATLAS_NAME) {
            continue;
        }

        if path.extension().and_then(|s| s.to_str()) == Some("png") {
            let stem = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s,
                None => continue,
            };

            let key = stem.to_string();

            let mut img = image::open(&path)
                .map_err(|e| {
                    CustomError::ValidationError(format!("Failed to load image {:?}: {}", path, e))
                })?
                .to_rgba8();

            image::imageops::flip_vertical_in_place(&mut img);

            packer.pack_own(key.clone(), img).map_err(|_| {
                CustomError::BuildError(format!("failed to pack sprite '{}'. Atlas full?", key))
            })?;
        }
    }

    Ok(())
}

fn write_atlas(
    ctx: &AtlasContext,
    packer: &TexturePacker<image::RgbaImage, String>,
) -> Result<AtlasOutput, CustomError> {
    let atlas_image = ImageExporter::export(packer, None)
        .map_err(|e| CustomError::BuildError(format!("Failed to export atlas: {}", e)))?;

    fs::create_dir_all(&ctx.atlas_dir)?;

    atlas_image
        .save(&ctx.atlas_path)
        .map_err(|_| CustomError::BuildError("Failed to save atlas".to_string()))?;

    ctx.info(&format!(
        "Atlas generated at {:?} ({}x{})",
        ctx.atlas_path,
        atlas_image.width(),
        atlas_image.height()
    ));

    Ok(AtlasOutput {
        width: atlas_image.width(),
        height: atlas_image.height(),
    })
}

fn should_repack(source_dir: &Path, target_file: &Path) -> Result<bool, CustomError> {
    if !target_file.exists() {
        return Ok(true);
    }

    if !source_dir.exists() {
        return Ok(false);
    }

    let target_metadata = fs::metadata(target_file).map_err(CustomError::IoError)?;
    let target_time = target_metadata.modified().map_err(CustomError::IoError)?;

    for entry in WalkDir::new(source_dir) {
        let entry = entry.map_err(|e| CustomError::IoError(e.into()))?;
        let path = entry.path();

        if path.is_dir() || path.file_name() == Some(std::ffi::OsStr::new(ATLAS_NAME)) {
            continue;
        }

        if path.extension().map_or(false, |ext| ext == "png") {
            let metadata = fs::metadata(path).map_err(CustomError::IoError)?;
            let modified_time = metadata.modified().map_err(CustomError::IoError)?;

            if modified_time > target_time {
                return Ok(true);
            }
        }
    }

    Ok(false)
}
