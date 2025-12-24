use crate::assets::generate_font_metadata;
use crate::error::CustomError;
use crate::manifest::{Manifest, update_manifest};
use crate::packer::pack_atlas;
use crate::shdc::get_or_install_shdc;
use colored::Colorize;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::{env, fs};

// general
const ASSETS_DIR: &str = "assets";
const IMAGES_DIR: &str = "assets/images";
const SHADERS_SRC: &str = "source/shaders/shader.glsl";
const SHADERS_OUT: &str = "source/shaders/shader.odin";
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
const SOKOL_LIB_DIR: &str = "source/libs/sokol";
const UTILS_DIR: &str = "utils";
// for checking whether sokol is compiled already
const SOKOL_APP_WASM: &str = "app/sokol_app_wasm_gl_release.a";
const SOKOL_APP_LINUX: &str = "app/sokol_app_linux_x64_gl_release.a";
const SOKOL_APP_MACOS: &str = "app/sokol_app_macos_arm64_gl_release.a";
const SOKOL_APP_WINDOWS: &str = "app/sokol_app_windows_x64_gl_release.a";
// manifest
const MANIFEST_NAME: &str = "bonsai.toml";
// emscripten
const EMSCRIPTEN_FLAGS: &str = "-sWASM_BIGINT \
-sWARN_ON_UNDEFINED_SYMBOLS=0 \
-sALLOW_MEMORY_GROWTH \
-sINITIAL_MEMORY=67108864 \
-sMAX_WEBGL_VERSION=2 \
-sASSERTIONS \
--shell-file source/core/platform/web/index.html \
--preload-file assets/images/atlas.png \
--preload-file assets/fonts";

pub struct BuildResult {
    pub executable_path: PathBuf,
}

fn prepare_resources(verbose: bool) -> Result<(), CustomError> {
    println!("{} Running pre-build tasks...", "[INFO]".green());
    check_dependencies()?;
    run_utils(verbose)?;
    update_manifest(Path::new("."))?;
    pack_atlas(Path::new(ASSETS_DIR), Path::new(IMAGES_DIR), verbose)?;
    generate_font_metadata()?;
    compile_shaders(verbose)?;
    Ok(())
}

fn get_c_libraries() -> Vec<String> {
    vec![
        "source/libs/sokol/app/sokol_app_wasm_gl_release.a".to_string(),
        "source/libs/sokol/glue/sokol_glue_wasm_gl_release.a".to_string(),
        "source/libs/sokol/gfx/sokol_gfx_wasm_gl_release.a".to_string(),
        "source/libs/sokol/shape/sokol_shape_wasm_gl_release.a".to_string(),
        "source/libs/sokol/log/sokol_log_wasm_gl_release.a".to_string(),
        "source/libs/sokol/gl/sokol_gl_wasm_gl_release.a".to_string(),
        "source/libs/stb/lib/stb_image_wasm.o".to_string(),
        "source/libs/stb/lib/stb_image_write_wasm.o".to_string(),
        "source/libs/stb/lib/stb_rect_pack_wasm.o".to_string(),
        "source/libs/stb/lib/stb_truetype_wasm.o".to_string(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

fn compile_shaders(verbose: bool) -> Result<(), CustomError> {
    if verbose {
        println!("{} Compiling shaders...", "[INFO]".green());
    }
    let shdc_path = get_or_install_shdc();

    let shader_format = if cfg!(target_os = "windows") {
        "glsl300es:hlsl4:glsl430"
    } else {
        "metal_macos:glsl300es:hlsl4:glsl430"
    };

    run_with_prefix(
        &shdc_path.to_string_lossy(),
        &[
            "-i",
            SHADERS_SRC,
            "-o",
            SHADERS_OUT,
            "-l",
            shader_format,
            "-f",
            "sokol_odin",
        ],
        "[SHDC]",
        colored::Color::Cyan,
    )?;

    Ok(())
}

fn compile_project(
    is_web_target: bool,
    config: &str,
    clean: bool,
    verbose: bool,
) -> Result<PathBuf, CustomError> {
    compile_sokol(is_web_target, clean, verbose)?;

    println!(
        "{} Compiling project for {}...",
        "[INFO]".green(),
        if is_web_target { "web" } else { "desktop" }
    );

    let (out_dir_str, binary_name) = if is_web_target {
        (BUILD_WEB_DIR, WEB_BINARY_NAME)
    } else {
        (BUILD_DESKTOP_DIR, DESKTOP_BINARY_NAME)
    };

    let out_dir = Path::new(out_dir_str);
    let out_path = out_dir.join(binary_name);

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

    let out_flag = format!("-out:{}", out_path.to_string_lossy());
    args.push(&out_flag);

    run_with_prefix(
        "odin",
        &args.iter().map(|s| s.as_ref()).collect::<Vec<&str>>(),
        "[ODIN]",
        colored::Color::Blue,
    )?;

    Ok(out_path)
}

pub fn build_desktop(config: &str, clean: bool, verbose: bool) -> Result<BuildResult, CustomError> {
    prepare_resources(verbose)?;

    let binary_path = compile_project(false, config, clean, verbose)?;

    if verbose {
        println!("{} Copying assets...", "[INFO]".green());
    }
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

pub fn build_web(config: &str, clean: bool, verbose: bool) -> Result<(), CustomError> {
    prepare_resources(verbose)?;

    let object_file = compile_project(true, config, clean, verbose)?;

    if verbose {
        println!("{} Copying runtime files...", "[INFO]".green());
    }
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

    if verbose {
        println!("{} Linking with Emscripten...", "[INFO]".green());
    }
    let emsdk_path = get_emsdk_path()?;

    let mut libraries = get_c_libraries();
    libraries.insert(0, object_file.to_string_lossy().to_string());

    let manifest_content =
        fs::read_to_string(MANIFEST_NAME).map_err(|e| CustomError::IoError(e))?;

    let manifest: Manifest = toml::from_str(&manifest_content)
        .map_err(|e| CustomError::ValidationError(format!("Invalid manifest: {}", e)))?;

    if !manifest.build.web_libs.is_empty() {
        println!(
            "  + Adding {} external libraries from manifest.",
            manifest.build.web_libs.len()
        );
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
    let out_html = out_dir.join("index.html").to_string_lossy().to_string();

    let emcc_cmd = format!("emcc -o {} {} {} -g", out_html, libs_str, EMSCRIPTEN_FLAGS);

    run_in_emsdk(&emcc_cmd, &emsdk_path)?;

    let binary_path = Path::new(BUILD_WEB_DIR).join(WEB_BINARY_NAME);
    let _ = fs::remove_file(binary_path);

    println!("{} Web build created in build/web", "[INFO]".green());
    Ok(())
}

pub fn clean_build(verbose: bool) -> Result<(), CustomError> {
    let build_dir = Path::new(BUILD_SRC);
    if build_dir.exists() {
        fs::remove_dir_all(build_dir)?;
        if verbose {
            println!("{} Cleaned build directory.", "[INFO]".green());
        }
    }

    let shader_output = Path::new(SHADERS_OUT);
    if shader_output.exists() {
        fs::remove_file(shader_output)?;
        if verbose {
            println!("{} Cleaned shader output.", "[INFO]".green());
        }
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

fn compile_sokol(is_web_target: bool, clean: bool, verbose: bool) -> Result<(), CustomError> {
    if verbose {
        println!("{} Compiling sokol...", "[INFO]".green());
    }

    let sokol_dir = Path::new(SOKOL_LIB_DIR);

    let (script_name, expected_output, shell, shell_flag) = if is_web_target {
        get_emsdk_path()?; // this is stupid. but it works.

        let output = SOKOL_APP_WASM;

        if cfg!(windows) {
            ("build_clibs_wasm.bat", output, "cmd", "/C")
        } else {
            ("build_clibs_wasm.sh", output, "sh", "-c")
        }
    } else {
        if cfg!(target_os = "windows") {
            ("build_clibs_windows.cmd", SOKOL_APP_WINDOWS, "cmd", "/C")
        } else if cfg!(target_os = "macos") {
            ("build_clibs_macos.sh", SOKOL_APP_MACOS, "sh", "-c")
        } else {
            ("build_clibs_linux.sh", SOKOL_APP_LINUX, "sh", "-c")
        }
    };

    let script_path = sokol_dir.join(script_name);
    let output_path = sokol_dir.join(expected_output);

    if !clean && output_path.exists() {
        if verbose {
            println!(
                "{} Sokol compilation skipped (already compiled).",
                "[INFO]".green()
            );
        }
        return Ok(());
    }

    if !script_path.exists() {
        return Err(CustomError::BuildError(format!(
            "Sokol build script not found at: {}",
            script_path.display()
        )));
    }

    let status = if cfg!(windows) {
        Command::new("cmd")
            .args(["/C", script_name])
            .current_dir(sokol_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
    } else {
        let cmd_string = format!("./{}", script_name);
        Command::new(shell)
            .args([shell_flag, &cmd_string])
            .current_dir(sokol_dir)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
    }
    .map_err(|e| CustomError::ProcessError(format!("Failed to run sokol script: {}", e)))?;

    if !status.success() {
        return Err(CustomError::BuildError(format!(
            "Sokol compilation failed: {}",
            script_name
        )));
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
    let emsdk_str = emsdk_path.to_string_lossy();

    let (shell, flag, command_string) = if cfg!(target_os = "windows") {
        (
            "cmd",
            "/C",
            format!("call \"{}\\emsdk_env.bat\" >nul && {}", emsdk_str, cmd),
        )
    } else {
        (
            "bash",
            "-c",
            format!("source \"{}/emsdk_env.sh\" && {}", emsdk_str, cmd),
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

fn run_utils(verbose: bool) -> Result<(), CustomError> {
    let utils_dir = Path::new(UTILS_DIR);
    if !utils_dir.exists() {
        return Ok(());
    }

    let utils_status = Command::new("odin")
        .args(&["run", "./utils"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|_| CustomError::BuildError("Failed to run utils".into()))?;

    if !utils_status.success() {
        return Err(CustomError::BuildError("Utils script failed".into()));
    }
    if verbose {
        println!("{} Running utility scripts...", "[INFO]".green());
    }
    for entry in fs::read_dir(utils_dir).map_err(CustomError::IoError)? {
        let entry = entry.map_err(CustomError::IoError)?;
        let path = entry.path();

        if path.is_dir() {
            continue;
        }

        if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
            match ext {
                "py" => {
                    run_with_prefix(
                        "python3",
                        &[path.to_str().unwrap()],
                        "[PYTHON]",
                        colored::Color::Yellow,
                    )
                    .or_else(|_| {
                        run_with_prefix(
                            "python",
                            &[path.to_str().unwrap()],
                            "[PYTHON]",
                            colored::Color::Yellow,
                        )
                    })?;
                }
                "rs" => {
                    println!(
                        "{} Running Rust script: {:?}",
                        "[RUST]".bright_red(),
                        path.file_name().unwrap()
                    );
                    run_rust_script(&path)?;
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
) -> Result<(), CustomError> {
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

    let stdout_thread = std::thread::spawn(move || {
        let stdout_reader = BufReader::new(stdout);
        for line in stdout_reader.lines() {
            if let Ok(l) = line {
                println!("{}", format!("{} {}", prefix_out, l).color(color));
            }
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        let stderr_reader = BufReader::new(stderr);
        for line in stderr_reader.lines() {
            if let Ok(l) = line {
                eprintln!("{}", format!("{} {}", prefix_err, l).color(color));
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
