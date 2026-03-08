use crate::Ui;
use crate::assets::{detect_native_size, generate_empty_sprite_metadata, generate_sprite_metadata, generate_font_metadata};
use crate::error::CustomError;
use std::collections::{BTreeSet, HashMap};
use std::fs::{self};
use std::path::{Path, PathBuf};
use texture_packer::{TexturePacker, TexturePackerConfig, exporter::ImageExporter};
use walkdir::WalkDir;
use std::io::Cursor;
use image::{Rgba, RgbaImage, ImageFormat};
use ttf_parser::Face;
use msdfgen::{Bitmap, FontExt, Framing, MsdfGeneratorConfig, Projection, Rgb, Vector2};
use fontdue::FontSettings;
use colored::Colorize;

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

pub struct HotReloadPayload {
    pub png_bytes: Vec<u8>,
    pub metadata_bin: Vec<u8>,
}

pub struct GlyphMetrics {
    pub x_offset: f32,
    pub y_offset: f32,
    pub advance: f32,
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

// msdf font packing
pub fn pack_font(font_path: &Path, font_name: &str, is_pixel: bool, native_size: u8, font_output_dir: &Path, ui: &Ui) -> Result<Option<HotReloadPayload>, CustomError> {
    let bin_path = font_output_dir.join(format!("{}.bin", font_name));
    let font_atlas_path = font_output_dir.join(format!("{}.png", font_name));

    if let Ok(source_meta) = std::fs::metadata(font_path) {
        if let (Ok(bin_meta), Ok(png_meta)) = (std::fs::metadata(&bin_path), std::fs::metadata(&font_atlas_path)) {
            if let (Ok(source_time), Ok(bin_time), Ok(png_time)) = (source_meta.modified(), bin_meta.modified(), png_meta.modified()) {
                if bin_time > source_time && png_time > source_time {
                    ui.status(&format!("Using cached font: {}", font_name));
                }

                let metadata_bin = std::fs::read(&bin_path).map_err(CustomError::IoError)?;
                let png_bytes = std::fs::read(&font_atlas_path).map_err(CustomError::IoError)?;

                return Ok(Some(HotReloadPayload {
                    png_bytes,
                    metadata_bin,
                }));
            }
        }
    }

    if ui.verbose {
        if is_pixel {
            ui.status(&format!("Packing pixel font: {} ({}px)...", font_name, native_size));
        } else {
            ui.status(&format!("Packing MSDF Vector font: {}...", font_name));
        }
    }

    let font_bytes = std::fs::read(font_path).map_err(|_| CustomError::BuildError("Failed to read font".into()))?;

    let config = TexturePackerConfig {
        max_width: 2048,
        max_height: 2048,
        allow_rotation: false,
        texture_padding: 8,
        border_padding: 8,
        trim: false,
        ..Default::default()
    };
    let mut packer = TexturePacker::new_skyline(config);
    let mut metrics_map: HashMap<char, GlyphMetrics> = HashMap::new();

    if is_pixel {
        let font = fontdue::Font::from_bytes(font_bytes.clone(), FontSettings::default())
            .map_err(|_| CustomError::BuildError("Failed to parse font with fontdue".into()))?;

        let mut blurry_warning_logged = false;

        for c in 32..127u8 {
            let ch = c as char;

            if ch == ' ' {
                let (metrics, _) = font.rasterize(ch, native_size as f32);
                metrics_map.insert(ch, GlyphMetrics{
                    x_offset: 0.0,
                    y_offset: 0.0,
                    advance: metrics.advance_width,
                });
                let mut img = RgbaImage::new(1, 1);
                img.put_pixel(0, 0, Rgba([255, 255, 255, 1]));
                packer.pack_own(ch.to_string(), img).unwrap();
                continue;
            } 

            let (metrics, bitmap) = font.rasterize(ch, native_size as f32);
            if metrics.width == 0 || metrics.height == 0 {
                continue;
            }

            metrics_map.insert(ch, GlyphMetrics {
                x_offset: metrics.xmin as f32,
                y_offset: metrics.ymin as f32,
                advance: metrics.advance_width,
            });

            if !blurry_warning_logged {
                for &alpha in &bitmap {
                    if alpha > 0 && alpha < 255 {
                        ui.message(&format!("{} Pixel font '{}' contains blurred/anti-aliased pixels at size {}px. Are you sure this is its native size?", 
                            "[WARNING]".red(), font_name, native_size));
                        detect_native_size(&font_bytes, &font_name, ui);
                        blurry_warning_logged = true;
                        break;
                    }
                }
            }

            let mut img = RgbaImage::new(metrics.width as u32, metrics.height as u32);
            for y in 0..metrics.height {
                for x in 0..metrics.width {
                    let mut alpha = bitmap[y * metrics.width + x];

                    if alpha > 127 {
                        alpha = 255;
                    } else {
                        alpha = 0;
                    }

                    img.put_pixel(x as u32, y as u32, Rgba([255, 255, 255, alpha]));
                }
            }

            image::imageops::flip_vertical_in_place(&mut img);
            packer.pack_own(ch.to_string(), img).unwrap();
        }
    } else {
        let face = Face::parse(&font_bytes, 0)
            .map_err(|_| CustomError::BuildError("Failed to parse font".into()))?;

        let px_size = 64.0;
        let scale = px_size / face.units_per_em() as f64;
    
        for c in 32..127u8 {
            let ch = c as char;

            if ch == ' ' {
                let fallback_advance = (face.units_per_em() / 3) as u16;
                let advance = face.glyph_index(ch)
                    .and_then(|id| face.glyph_hor_advance(id))
                    .unwrap_or(fallback_advance) as f64 * scale;
                metrics_map.insert(ch, GlyphMetrics {
                    x_offset: 0.0,
                    y_offset: 0.0,
                    advance: advance as f32,
                });
                let mut img = RgbaImage::new(1, 1);
                img.put_pixel(0, 0, Rgba([255, 255, 255, 1]));
                packer.pack_own(ch.to_string(), img).unwrap();
                continue;
            }

            if let Some(glyph_id) = face.glyph_index(ch) {
                let mut shape = face.glyph_shape(glyph_id).unwrap_or_default();
                let bounds = shape.get_bound();

                if bounds.left >= bounds.right || bounds.bottom >= bounds.top {
                    continue;
                }

                let advance = face.glyph_hor_advance(glyph_id).unwrap_or(0) as f64 * scale;

                let padding_px = 8.0;
                let padding_font_units = padding_px / scale;

                let width_px = ((bounds.right - bounds.left) * scale).ceil() as u32 + (padding_px as u32 * 2);
                let height_px = ((bounds.top - bounds.bottom) * scale).ceil() as u32 + (padding_px as u32 * 2);

                metrics_map.insert(ch, GlyphMetrics {
                    x_offset: (bounds.left * scale) as f32 - padding_px as f32,
                    y_offset: (bounds.bottom * scale) as f32 - padding_px as f32,
                    advance: advance as f32,
                });

                let translation = Vector2::new(-bounds.left + padding_font_units, -bounds.bottom + padding_font_units);
                let projection = Projection::new(Vector2::new(scale, scale), translation);

                let framing = Framing {
                    projection,
                    range: padding_font_units,
                };

                let mut msdf_bitmap = Bitmap::<Rgb<f32>>::new(width_px, height_px);

                shape.edge_coloring_simple(3.0, 0);

                shape.generate_msdf(&mut msdf_bitmap, &framing, &MsdfGeneratorConfig::default());

                let mut img = RgbaImage::new(width_px, height_px);
                for y in 0..height_px {
                    for x in 0..width_px {
                        let pixel = msdf_bitmap.pixel(x, y);
                        img.put_pixel(x, y, Rgba([
                            (pixel.r * 255.0) as u8,
                            (pixel.g * 255.0) as u8,
                            (pixel.b * 255.0) as u8,
                            255
                        ]));
                    }
                }

                packer.pack_own(ch.to_string(), img).unwrap();
            }
        }
    }

    let atlas_image = ImageExporter::export(&packer, None)
        .map_err(|e| CustomError::BuildError(format!("Failed to export MSDF atlas: {}", e)))?;

    let mut png_bytes: Vec<u8> = Vec::new();
    atlas_image.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .map_err(|_| CustomError::BuildError("Failed to encode MSDF PNG to memory".into()))?;

    let metadata_bin = generate_font_metadata(&packer, atlas_image.width(), atlas_image.height(), is_pixel, native_size, &metrics_map)?;

    let bin_path = font_output_dir.join(format!("{}.bin", font_name));
    if let Some(parent) = bin_path.parent() {
        fs::create_dir_all(parent).map_err(CustomError::IoError)?;
    }
    fs::write(bin_path, &metadata_bin).map_err(CustomError::IoError)?;

    let font_atlas_path = font_output_dir.join(format!("{}.png", font_name));
    atlas_image.save(&font_atlas_path)
        .map_err(|_| CustomError::BuildError(format!("Failed to save font atlas: {}", font_name)))?;

    if ui.verbose {
        ui.log(&format!(
            "MSDF Font {} Atlas generated ({}x{})",
            font_name,
            atlas_image.width(),
            atlas_image.height(),
        ));
    }

    Ok(Some(HotReloadPayload {
        png_bytes,
        metadata_bin,
    }))
}

pub fn pack_atlas(assets_dir: &Path, atlas_dir: &Path, ui: &Ui) -> Result<Option<HotReloadPayload>, CustomError> {
    let ctx = AtlasContext::new(assets_dir, atlas_dir);

    if !should_repack(&ctx.images_dir, &ctx.atlas_path)? && ui.verbose {
        ui.log("Atlas is up to date. Skipping packing.");
        return Ok(None);
    }

    let sorted_files = get_sorted_image_files(&ctx.images_dir)?;
    if sorted_files.is_empty() {
        generate_empty_sprite_metadata()?;
        if ui.verbose {
            ui.log("No images to pack in assets directory. Skipping packing.");
        }
        return Ok(None);
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
    let mut extruded_sprites: BTreeSet<String> = BTreeSet::new();
    process_images(&ctx, &sorted_files, &mut packer, &mut extruded_sprites, ui)?;
    let (output, png_bytes) = write_atlas(&ctx, &packer, ui)?;
    let metadata_bin = generate_sprite_metadata(&packer, output.width, output.height, &extruded_sprites)?;

    Ok(Some(HotReloadPayload {
        png_bytes,
        metadata_bin,
    }))
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
    extruded_sprites: &mut BTreeSet<String>,
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
) -> Result<(AtlasOutput, Vec<u8>), CustomError> {
    let atlas_image = ImageExporter::export(packer, None)
        .map_err(|e| CustomError::BuildError(format!("Failed to export atlas: {}", e)))?;

    fs::create_dir_all(&ctx.atlas_dir)?;

    atlas_image
        .save(&ctx.atlas_path)
        .map_err(|_| CustomError::BuildError("Failed to save atlas".to_string()))?;

    let mut png_bytes: Vec<u8> = Vec::new();
    atlas_image.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .map_err(|_| CustomError::BuildError("Failed to encode PNG to memory".to_string()))?;

    if ui.verbose {
        ui.log(&format!(
            "Atlas generated at {:?} ({}x{})",
            ctx.atlas_path,
            atlas_image.width(),
            atlas_image.height()
        ));
    }

    Ok((AtlasOutput {
        width: atlas_image.width(),
        height: atlas_image.height(),
    }, png_bytes))
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
