use crate::Ui;
use crate::assets::generate_assets;
use crate::error::CustomError;
use crate::manifest::{Manifest, update_manifest};
use crate::packer::pack_atlas;
use crate::shdc::get_or_install_shdc;
use crate::sokol;
use colored::Colorize;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs};
use walkdir::WalkDir;

// general
const ASSETS_DIR: &str = "assets";
const ATLAS_DIR: &str = "bonsai/core/render/atlas";
const BONSAI_DIR: &str = "./bonsai";
const GAME_DIR: &str = "./source/game";
const SHADERS_CACHE_DIR: &str = ".bonsai/cache/shaders";
const SHADERS_INCLUDE_SRC: &str = "bonsai/shaders/include";
const SHADERS_CORE_VS_NAME: &str = "shader_vs_core/shader_vs_core.glsl";
const SHADERS_CORE_FS_NAME: &str = "shader_fs_core/shader_fs_core.glsl";
const SHADERS_HEADER_NAME: &str = "shader_header/shader_header.glsl";
const SHADERS_UTILS_NAME: &str = "shader_utils/shader_utils.glsl";
const SHADERS_BONSAI_SRC: &str = "bonsai/shaders/shader.glsl";
const SHADERS_BONSAI_OUT: &str = "bonsai/shaders/shader.odin";
const SHADERS_GAME_SRC: &str = "source/game/shaders";
const SOURCE_DIR: &str = "source";
// build
const BUILD_SRC: &str = "build";
const BUILD_DESKTOP_DIR: &str = "build/desktop";
const BUILD_WEB_DIR: &str = "build/web";
const DESKTOP_BINARY_NAME: &str = if cfg!(windows) {
    "game_desktop.exe"
} else {
    "game_desktop.bin"
};
const WEB_BINARY_NAME: &str = "game.wasm.o";
const UTILS_DIR: &str = "utils";
// manifest
const MANIFEST_NAME: &str = "bonsai.toml";
// emscripten
const EMSCRIPTEN_FLAGS: &str = "-sWASM_BIGINT \
-sWARN_ON_UNDEFINED_SYMBOLS=0 \
-sALLOW_MEMORY_GROWTH \
-sINITIAL_MEMORY=67108864 \
-sMAX_WEBGL_VERSION=2 \
-sASSERTIONS \
--shell-file bonsai/core/platform/web/index.html \
--preload-file bonsai/core/render/atlas \
--preload-file assets/audio \
--preload-file assets/fonts \
--preload-file bonsai/core/ui/PixelCode.ttf";

pub struct BuildResult {
    pub executable_path: PathBuf,
}

fn prepare_resources(ui: &Ui) -> Result<(), CustomError> {
    if ui.verbose {
        ui.status("Running pre-build tasks...");
    }
    check_dependencies()?;
    run_utils(ui)?;
    update_manifest(Path::new("."), ui)?;
    pack_atlas(Path::new(ASSETS_DIR), Path::new(ATLAS_DIR), ui)?;
    generate_assets()?;
    compile_shaders(ui)?;
    Ok(())
}

fn get_c_libraries() -> Vec<String> {
    vec![
        "bonsai/libs/sokol/app/sokol_app_wasm_gl_release.a".to_string(),
        "bonsai/libs/sokol/glue/sokol_glue_wasm_gl_release.a".to_string(),
        "bonsai/libs/sokol/gfx/sokol_gfx_wasm_gl_release.a".to_string(),
        "bonsai/libs/sokol/shape/sokol_shape_wasm_gl_release.a".to_string(),
        "bonsai/libs/sokol/log/sokol_log_wasm_gl_release.a".to_string(),
        "bonsai/libs/sokol/gl/sokol_gl_wasm_gl_release.a".to_string(),
        "bonsai/libs/sokol/audio/sokol_audio_wasm_gl_release.a".to_string(),
        "bonsai/libs/stb/lib/stb_image_wasm.o".to_string(),
        "bonsai/libs/stb/lib/stb_image_write_wasm.o".to_string(),
        "bonsai/libs/stb/lib/stb_rect_pack_wasm.o".to_string(),
        "bonsai/libs/stb/lib/stb_truetype_wasm.o".to_string(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn compile_shaders(ui: &Ui) -> Result<(), CustomError> {
    let shdc_path = get_or_install_shdc(ui);
    let shdc_str = shdc_path.to_string_lossy();

    let cache_dir = Path::new(SHADERS_CACHE_DIR);
    if cache_dir.exists() {
        fs::remove_dir_all(cache_dir).map_err(|e| CustomError::IoError(e))?;
    }
    fs::create_dir_all(cache_dir).map_err(|e| CustomError::IoError(e))?;

    let include_dir = Path::new(SHADERS_INCLUDE_SRC);
    fs::copy(
        include_dir.join(SHADERS_CORE_VS_NAME),
        cache_dir.join("shader_vs_core.glsl"),
    )?;
    fs::copy(
        include_dir.join(SHADERS_CORE_FS_NAME),
        cache_dir.join("shader_fs_core.glsl"),
    )?;
    fs::copy(
        include_dir.join(SHADERS_UTILS_NAME),
        cache_dir.join("shader_utils.glsl"),
    )?;
    fs::copy(
        include_dir.join(SHADERS_HEADER_NAME),
        cache_dir.join("shader_header.glsl"),
    )?;

    let shader_format = if cfg!(target_os = "windows") {
        "glsl300es:hlsl4:glsl430"
    } else {
        "metal_macos:glsl300es:hlsl4:glsl430"
    };

    let compile_shader_cached = |src_path: &Path,
                                 out_path: &Path,
                                 log_prefix: &str,
                                 color: colored::Color|
     -> Result<(), CustomError> {
        let filename = src_path.file_name().unwrap();
        let cached_path = cache_dir.join(filename);
        let cached_path_str = cached_path.to_str().unwrap();
        let out_path_str = out_path.to_str().unwrap();

        fs::copy(src_path, &cached_path).map_err(|e| CustomError::IoError(e))?;

        ui.status(&format!("Compiling shader: {}", src_path.to_string_lossy()));

        run_with_prefix(
            &shdc_str,
            &[
                "-i",
                cached_path_str,
                "-o",
                out_path_str,
                "-l",
                shader_format,
                "-f",
                "sokol_odin",
            ],
            log_prefix,
            color,
            ui,
        )
    };

    if !should_skip(Path::new(SHADERS_BONSAI_SRC), Path::new(SHADERS_BONSAI_OUT))? {
        compile_shader_cached(
            Path::new(SHADERS_BONSAI_SRC),
            Path::new(SHADERS_BONSAI_OUT),
            "[CORE SHDC]",
            colored::Color::Cyan,
        )?;
    } else if ui.verbose {
        ui.log("Core shader compilation skipped (already compiled).");
    }

    let walker = WalkDir::new(SHADERS_GAME_SRC).into_iter();

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        match path.extension().and_then(|s| s.to_str()) {
            Some("glsl") | Some("vert") | Some("frag") => {}
            _ => continue,
        }

        let output_path = path.with_extension("odin");
        if should_skip(path, &output_path)? {
            continue;
        }

        compile_shader_cached(
            path,
            &output_path,
            "[GAME SHDC]",
            colored::Color::BrightBlue,
        )?;
    }

    Ok(())
}

fn to_emcc_path(path: &Path) -> String {
    path.to_str().unwrap_or("").replace("\\", "/")
}

fn compile_project(
    is_web_target: bool,
    config: &str,
    clean: bool,
    ui: &Ui,
) -> Result<PathBuf, CustomError> {
    let is_debug = config == "debug";
    sokol::compile_sokol(is_web_target, is_debug, clean, ui)?;
    // compile_sokol(is_web_target, clean, ui)?;

    let (out_dir_str, binary_name) = if is_web_target {
        (BUILD_WEB_DIR, WEB_BINARY_NAME)
    } else {
        (BUILD_DESKTOP_DIR, DESKTOP_BINARY_NAME)
    };

    let out_dir = Path::new(out_dir_str);
    let out_path = out_dir.join(binary_name);
    let out_clean_str = to_emcc_path(&out_path);
    let out_clean_path = Path::new(&out_clean_str).to_path_buf();

    if !out_dir.exists() {
        fs::create_dir_all(out_dir).map_err(CustomError::IoError)?;
    }

    let mut args = vec!["build", SOURCE_DIR, "-vet", "-strict-style"];

    if is_web_target {
        args.push("-target:js_wasm32");
        args.push("-build-mode:obj");
    }

    if config == "debug" {
        args.push("-debug");
    } else {
        args.push("-o:speed");
        args.push("-no-bounds-check");
    }

    let out_flag = format!("-out:{}", out_clean_path.to_string_lossy());
    args.push(&out_flag);

    let bonsai_collection_flag = format!("-collection:bonsai={}", BONSAI_DIR);
    args.push(&bonsai_collection_flag);

    let game_collection_flag = format!("-collection:game={}", GAME_DIR);
    args.push(&game_collection_flag);

    run_with_prefix(
        "odin",
        &args.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
        "[ODIN]",
        colored::Color::Blue,
        ui,
    )?;

    Ok(out_clean_path)
}

pub fn build_desktop(config: &str, clean: bool, ui: &Ui) -> Result<BuildResult, CustomError> {
    prepare_resources(ui)?;

    let binary_path = compile_project(false, config, clean, ui)?;

    ui.status("Copying assets...");
    let out_dir = binary_path.parent().unwrap();
    let assets_src = Path::new(ASSETS_DIR);
    let assets_dest = out_dir.join(ASSETS_DIR);

    if assets_src.exists() {
        copy_dir_recursive(assets_src, &assets_dest)?;
    }

    Ok(BuildResult {
        executable_path: binary_path,
    })
}

pub fn build_web(config: &str, clean: bool, ui: &Ui) -> Result<(), CustomError> {
    prepare_resources(ui)?;

    let object_file = compile_project(true, config, clean, ui)?;

    ui.status("Copying runtime files...");
    let out_dir = object_file.parent().unwrap();

    let odin_root_out = Command::new("odin")
        .arg("root")
        .output()
        .map_err(|_| CustomError::BuildError("Could not find 'odin' to get root path".into()))?;

    let odin_root_str = String::from_utf8_lossy(&odin_root_out.stdout)
        .trim()
        .to_string();
    let odin_root = Path::new(&odin_root_str);

    let odin_js_src = odin_root.join("core/sys/wasm/js/odin.js");
    let odin_js_dest = out_dir.join("odin.js");

    fs::copy(&odin_js_src, &odin_js_dest).map_err(|e| CustomError::IoError(e))?;

    let assets_src = Path::new(ASSETS_DIR);
    let assets_dest = out_dir.join(ASSETS_DIR);
    if assets_src.exists() {
        copy_dir_recursive(assets_src, &assets_dest).map_err(CustomError::IoError)?;
    }

    ui.status("Linking with Emscripten...");
    let emsdk_path = get_emsdk_path()?;

    let mut libraries = get_c_libraries();
    libraries.insert(0, object_file.to_string_lossy().to_string());

    let manifest_content =
        fs::read_to_string(MANIFEST_NAME).map_err(|e| CustomError::IoError(e))?;

    let manifest: Manifest = toml_edit::de::from_str(&manifest_content)
        .map_err(|e| CustomError::ValidationError(format!("Invalid manifest: {}", e)))?;

    if !manifest.build.web_libs.is_empty() {
        ui.message(&format!(
            "  + Adding {} external libraries from manifest.",
            manifest.build.web_libs.len()
        ));
        for lib in manifest.build.web_libs {
            let lib_path = Path::new(&lib);
            if !lib_path.exists() {
                return Err(CustomError::ValidationError(format!(
                    "External library not found: {}",
                    lib
                )));
            }
            libraries.push(lib.to_string());
        }
    }

    let libs_str = libraries.join(" ");
    let out_html = to_emcc_path(out_dir.join("index.html").as_path());

    let emcc_cmd = format!("emcc -o {} {} {} -g", out_html, libs_str, EMSCRIPTEN_FLAGS);

    run_in_emsdk(&emcc_cmd, &emsdk_path)?;

    let binary_path = Path::new(BUILD_WEB_DIR).join(WEB_BINARY_NAME);
    let _ = fs::remove_file(binary_path);

    ui.success("Web build created in build/web.");
    Ok(())
}

//TODO: clean user shaders
pub fn clean_build(ui: &Ui) -> Result<(), CustomError> {
    let build_dir = Path::new(BUILD_SRC);
    if build_dir.exists() {
        fs::remove_dir_all(build_dir)?;
        ui.log("Cleaned build directory.");
    }

    let shader_output = Path::new(SHADERS_BONSAI_OUT);
    if shader_output.exists() {
        fs::remove_file(shader_output)?;
        ui.log("Cleaned shader output.");
    }

    let _ = fs::remove_dir_all("utils/__pycache__"); // python scripts
    let _ = fs::remove_dir_all("utils/target"); //rust scripts

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if !dest.exists() {
        fs::create_dir_all(dest)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if path.is_dir() {
            copy_dir_recursive(&path, &dest_path)?;
        } else {
            let should_copy = if dest_path.exists() {
                let src_meta = fs::metadata(&path)?;
                let dest_meta = fs::metadata(&dest_path)?;
                src_meta.modified()? > dest_meta.modified()?
            } else {
                true
            };

            if should_copy {
                fs::copy(&path, &dest_path)?;
            }
        }
    }
    Ok(())
}

fn get_emsdk_path() -> Result<PathBuf, CustomError> {
    if let Ok(path) = env::var("EMSDK") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Ok(path);
        }
    }

    if let Ok(home) = env::var("HOME").or_else(|_| env::var("USERPROFILE")) {
        let home_path = PathBuf::from(home);

        let common_paths = ["repos/emsdk", "emsdk", "tools/emsdk", ".emsdk"]; // a little bit hacky, but might help someone

        for sub_path in common_paths {
            let attempt = home_path.join(sub_path);
            if attempt.join("emsdk_env.sh").exists() || attempt.join("emsdk_env.bat").exists() {
                return Ok(attempt);
            }
        }
    }

    Err(CustomError::BuildError(
        "Could not find Emscripten SDK.\n\
        Please install it (https://emscripten.org/docs/getting_started/downloads.html)\n\
        and set the 'EMSDK' environment variable to its installation folder"
            .to_string(),
    ))
}

fn run_in_emsdk(cmd: &str, emsdk_path: &Path) -> Result<(), CustomError> {
    let clean_emsdk_path = to_emcc_path(emsdk_path);

    let (shell, flag, command_string) = if cfg!(target_os = "windows") {
        (
            "cmd",
            "/C",
            format!("call {}/emsdk_env.bat >nul && {}", clean_emsdk_path, cmd),
        )
    } else {
        (
            "bash",
            "-c",
            format!("source \"{}/emsdk_env.sh\" && {}", clean_emsdk_path, cmd),
        )
    };

    let status = Command::new(shell)
        .env("EMSDK_QUIET", "1")
        .arg(flag)
        .arg(command_string)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| {
            CustomError::ProcessError(format!("Failed to run Emscripten command: {}", e))
        })?;

    if !status.success() {
        return Err(CustomError::BuildError(format!(
            "Emscripten command failed: {}",
            cmd
        )));
    }

    Ok(())
}

fn run_utils(ui: &Ui) -> Result<(), CustomError> {
    let utils_path = Path::new(UTILS_DIR);
    if !utils_path.exists() {
        return Ok(());
    }

    if ui.verbose {
        ui.status("Scanning for utility scripts...");
    }

    visit_utility_dir(utils_path, ui)?;

    Ok(())
}

fn visit_utility_dir(dir: &Path, ui: &Ui) -> Result<(), CustomError> {
    for entry in fs::read_dir(dir).map_err(CustomError::IoError)? {
        let entry = entry.map_err(CustomError::IoError)?;
        let path = entry.path();

        if path.is_dir() {
            visit_utility_dir(&path, ui)?;
            continue;
        }

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            let path_str = path
                .to_str()
                .ok_or(CustomError::ValidationError("Invalid UTF-8 path".into()))?;
            match ext {
                "py" => {
                    run_with_prefix(
                        "python3",
                        &[path_str],
                        "[PYTHON]",
                        colored::Color::Yellow,
                        ui,
                    )
                    .or_else(|_| {
                        run_with_prefix(
                            "python",
                            &[path_str],
                            "[PYTHON]",
                            colored::Color::Yellow,
                            ui,
                        )
                    })?;
                }
                "rs" => {
                    ui.message(&format!(
                        "{} Running Rust script: {:?}",
                        "[RUST]".bright_red(),
                        path_str
                    ));
                    run_rust_script(&path)?;
                }
                "odin" => {
                    run_with_prefix(
                        "odin",
                        &["run", path_str, "-file"],
                        "[ODIN]",
                        colored::Color::Blue,
                        ui,
                    )?;
                }
                _ => {}
            }
        }
    }

    Ok(())
}

fn run_rust_script(path: &Path) -> Result<(), CustomError> {
    let file_stem = path.file_stem().unwrap().to_str().unwrap();

    let out_name = if cfg!(windows) {
        format!("{}.exe", file_stem)
    } else {
        format!("{}.bin", file_stem)
    };
    let out_path = path.parent().unwrap().join(&out_name);

    let status = Command::new("rustc")
        .arg(path)
        .arg("-o")
        .arg(&out_path)
        .status()
        .map_err(|e| CustomError::ProcessError(format!("Failed to compile rust script: {}", e)))?;

    if !status.success() {
        return Err(CustomError::BuildError(format!(
            "Failed to compile {:?}",
            path
        )));
    }

    let status = Command::new(&out_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    let _ = fs::remove_file(&out_path);

    if cfg!(windows) {
        let _ = fs::remove_file(out_path.with_extension("pdb"));
    }

    if !status.map_err(CustomError::IoError)?.success() {
        return Err(CustomError::BuildError(format!(
            "Rust script {:?} failed",
            path
        )));
    }

    Ok(())
}

fn check_dependencies() -> Result<(), CustomError> {
    if Command::new("odin").arg("version").output().is_err() {
        return Err(CustomError::BuildError(
            "Odin compiler not found in PATH. Please install it from https://odin-lang.org/docs/install".to_string()
        ));
    }

    Ok(())
}

fn run_with_prefix(
    cmd: &str,
    args: &[&str],
    prefix: &str,
    color: colored::Color,
    ui: &Ui,
) -> Result<(), CustomError> {
    if ui.verbose {
        ui.message(&format!(
            "{} Running script with arguments: {}...",
            prefix,
            args.join(" ")
        ));
    }

    let mut child = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CustomError::ProcessError(format!("Failed to start {}: {}", cmd, e)))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let prefix_out = prefix.to_string();
    let prefix_err = prefix.to_string();

    let ui_clone_out = ui.clone();
    let ui_clone_err = ui.clone();

    let stdout_thread = std::thread::spawn(move || {
        let stdout_reader = BufReader::new(stdout);
        for line in stdout_reader.lines() {
            if let Ok(l) = line {
                ui_clone_out.message(&format!("{}", format!("{} {}", prefix_out, l).color(color)));
            }
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        let stderr_reader = BufReader::new(stderr);
        for line in stderr_reader.lines() {
            if let Ok(l) = line {
                ui_clone_err.error(&format!("{}", format!("{} {}", prefix_err, l).color(color)));
            }
        }
    });

    let status = child
        .wait()
        .map_err(|e| CustomError::ProcessError(format!("Failed to wait for {}: {}", cmd, e)))?;

    stdout_thread.join().unwrap();
    stderr_thread.join().unwrap();

    if !status.success() {
        return Err(CustomError::BuildError(format!("{} failed", cmd)));
    }

    Ok(())
}

fn should_skip(src: &Path, out: &Path) -> Result<bool, CustomError> {
    if !out.exists() {
        return Ok(false);
    }

    let src_meta = fs::metadata(src)?;
    let out_meta = fs::metadata(out)?;

    let src_time = src_meta.modified()?;
    let out_time = out_meta.modified()?;

    Ok(src_time <= out_time)
}
