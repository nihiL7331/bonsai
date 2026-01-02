use crate::Ui;
use crate::assets::generate_sprite_metadata;
use crate::error::CustomError;
use std::collections::HashSet;
use std::fs::{self};
use std::path::{Path, PathBuf};
use texture_packer::{TexturePacker, TexturePackerConfig, exporter::ImageExporter};
use walkdir::WalkDir;

const ATLAS_NAME: &str = "atlas.png";
const IMAGES_DIR_NAME: &str = "images";
const TILESETS_DIR_NAME: &str = "tilesets";
const DEFAULT_TILE_SIZE: u32 = 16;

struct AtlasContext {
    images_dir: PathBuf,
    tilesets_dir: PathBuf,
    atlas_path: PathBuf,
    atlas_dir: PathBuf,
}

struct AtlasOutput {
    width: u32,
    height: u32,
}

impl AtlasContext {
    fn new(assets_dir: &Path, atlas_dir: &Path) -> Self {
        let images_dir = assets_dir.join(IMAGES_DIR_NAME);
        let tilesets_dir = images_dir.join(TILESETS_DIR_NAME);
        let atlas_dir = PathBuf::from(atlas_dir);
        let atlas_path = atlas_dir.join(ATLAS_NAME);

        Self {
            images_dir,
            tilesets_dir,
            atlas_path,
            atlas_dir,
        }
    }
}

pub fn pack_atlas(assets_dir: &Path, atlas_dir: &Path, ui: &Ui) -> Result<(), CustomError> {
    let ctx = AtlasContext::new(assets_dir, atlas_dir);

    if !should_repack(&ctx.images_dir, &ctx.atlas_path)? && ui.verbose {
        ui.log("Atlas is up to date. Skipping packing.");
        return Ok(());
    }

    if ui.verbose {
        ui.status("Packing texture atlas...");
    }

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
    let mut extruded_sprites: HashSet<String> = HashSet::new();
    let sorted_files = get_sorted_image_files(&ctx.images_dir)?;
    process_images(&ctx, &sorted_files, &mut packer, &mut extruded_sprites, ui)?;
    let output = write_atlas(&ctx, &packer, ui)?;
    generate_sprite_metadata(&packer, output.width, output.height, &extruded_sprites)?;

    Ok(())
}

fn get_sorted_image_files(dir: &Path) -> Result<Vec<PathBuf>, CustomError> {
    let mut paths: Vec<PathBuf> = Vec::new();

    if !dir.exists() {
        return Ok(paths);
    }

    for entry in WalkDir::new(dir) {
        let entry = entry.map_err(|e| CustomError::IoError(e.into()))?;
        let path = entry.path();

        if path.is_dir() {
            continue;
        }
        if path.file_name().and_then(|s| s.to_str()) == Some(ATLAS_NAME) {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) == Some("png") {
            paths.push(path.to_path_buf());
        }
    }

    paths.sort();
    Ok(paths)
}

fn process_images(
    ctx: &AtlasContext,
    files: &[PathBuf],
    packer: &mut TexturePacker<image::RgbaImage, String>,
    extruded_sprites: &mut HashSet<String>,
    ui: &Ui,
) -> Result<(), CustomError> {
    for path in files {
        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap();
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap()
            .to_string();

        let mut img = image::open(path)
            .map_err(|e| CustomError::ValidationError(format!("Failed to load {:?}: {}", path, e)))?
            .to_rgba8();

        let is_tileset = path.starts_with(&ctx.tilesets_dir);

        if is_tileset {
            if ui.verbose {
                ui.log(&format!("Slicing tileset found: {}", file_name));
            }

            let (tile_w, tile_h) = parse_grid_size_from_name(&file_stem)
                .unwrap_or((DEFAULT_TILE_SIZE, DEFAULT_TILE_SIZE));

            let cols = img.width() / tile_w;
            let rows = img.height() / tile_h;

            for y in 0..rows {
                for x in 0..cols {
                    let sub_img =
                        image::imageops::crop_imm(&img, x * tile_w, y * tile_h, tile_w, tile_h)
                            .to_image();

                    let mut final_tile = sub_img;
                    image::imageops::flip_vertical_in_place(&mut final_tile);

                    let extruded_tile = extrude_tile(&final_tile);

                    let tile_index = x + (y * cols);
                    let key = format!("{}_{}", file_stem, tile_index);

                    packer.pack_own(key.clone(), extruded_tile).map_err(|_| {
                        CustomError::BuildError(format!(
                            "Failed to pack tile '{}'. Atlas full?",
                            key
                        ))
                    })?;

                    extruded_sprites.insert(key);
                }
            }
        } else {
            image::imageops::flip_vertical_in_place(&mut img);
            packer.pack_own(file_stem.clone(), img).map_err(|_| {
                CustomError::BuildError(format!(
                    "Failed to pack sprite '{}'. Atlas full?",
                    file_stem
                ))
            })?;
        }
    }

    Ok(())
}

//HACK: extrude edges of tiles by one pixel to ensure not getting tile seams
fn extrude_tile(img: &image::RgbaImage) -> image::RgbaImage {
    let (w, h) = img.dimensions();
    let mut new_img = image::RgbaImage::new(w + 2, h + 2);
    image::imageops::overlay(&mut new_img, img, 1, 1);

    for x in 0..w {
        let top_pixel = *img.get_pixel(x, 0);
        let bottom_pixel = *img.get_pixel(x, h - 1);
        new_img.put_pixel(x + 1, 0, top_pixel);
        new_img.put_pixel(x + 1, h + 1, bottom_pixel);
    }

    for y in 0..h {
        let left_pixel = *img.get_pixel(0, y);
        let right_pixel = *img.get_pixel(w - 1, y);

        new_img.put_pixel(0, y + 1, left_pixel);
        new_img.put_pixel(w + 1, y + 1, right_pixel);
    }

    new_img.put_pixel(0, 0, *img.get_pixel(0, 0));
    new_img.put_pixel(w + 1, 0, *img.get_pixel(w - 1, 0));
    new_img.put_pixel(0, h + 1, *img.get_pixel(0, h - 1));
    new_img.put_pixel(w + 1, h + 1, *img.get_pixel(w - 1, h - 1));

    new_img
}

fn parse_grid_size_from_name(name: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = name.split('_').collect();
    if let Some(last) = parts.last() {
        if let Some((w, h)) = last.split_once('x') {
            if let (Ok(width), Ok(height)) = (w.parse::<u32>(), h.parse::<u32>()) {
                return Some((width, height));
            }
        }
    }
    None
}

fn write_atlas(
    ctx: &AtlasContext,
    packer: &TexturePacker<image::RgbaImage, String>,
    ui: &Ui,
) -> Result<AtlasOutput, CustomError> {
    let atlas_image = ImageExporter::export(packer, None)
        .map_err(|e| CustomError::BuildError(format!("Failed to export atlas: {}", e)))?;

    fs::create_dir_all(&ctx.atlas_dir)?;

    atlas_image
        .save(&ctx.atlas_path)
        .map_err(|_| CustomError::BuildError("Failed to save atlas".to_string()))?;

    if ui.verbose {
        ui.log(&format!(
            "Atlas generated at {:?} ({}x{})",
            ctx.atlas_path,
            atlas_image.width(),
            atlas_image.height()
        ));
    }

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
