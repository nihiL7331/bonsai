use crate::Ui;
use crate::error::CustomError;
use rayon::prelude::*;
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

const SOKOL_LIB_DIR: &str = "bonsai/libs/sokol";

const SOKOL_MODULES: &[&str] = &[
    "sokol_log",
    "sokol_gfx",
    "sokol_app",
    "sokol_glue",
    "sokol_time",
    "sokol_audio",
    "sokol_debugtext",
    "sokol_shape",
    "sokol_gl",
];

fn clean_dir(path: &Path) {
    if !path.exists() {
        return;
    }

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let file_path = entry.path();

            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                if ext == "odin" {
                    continue;
                }

                let _ = fs::remove_file(&file_path);
            }
        }
    }
}

pub fn compile_sokol(
    is_web_target: bool,
    is_debug: bool,
    clean: bool,
    ui: &Ui,
) -> Result<(), CustomError> {
    if is_web_target {
        compile_sokol_wasm(clean, ui)?;
        return Ok(());
    }

    if cfg!(windows) {
        let check = Command::new("cl")
            .arg("/?")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        if check.is_err() {
            return Err(CustomError::BuildError(
                "The 'cl' command (MSVC compiler) was not found.\n\
                For Sokol compilation on Windows, you must run this tool from the \
                'Visual Studio Developer Command Prompt' or a terminal with Build Tools initialized"
                    .to_string(),
            ));
        }
    }

    let sokol_dir = Path::new(SOKOL_LIB_DIR);
    if !sokol_dir.exists() {
        return Err(CustomError::BuildError(format!(
            "Sokol directory not found: {}",
            sokol_dir.display()
        )));
    }

    let os = env::consts::OS;
    let profile = if is_debug { "Debug" } else { "Release" };
    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        "arm64"
    };
    let backends = match os {
        "windows" => vec![
            ("D3D11", "SOKOL_D3D11", "d3d11"),
            ("GL", "SOKOL_GLCORE", "gl"),
        ],
        "macos" => vec![
            ("Metal", "SOKOL_METAL", "metal"),
            ("GL", "SOKOL_GLCORE", "gl"),
        ],
        "linux" => vec![("GL", "SOKOL_GLCORE", "gl")],
        _ => return Err(CustomError::BuildError(format!("Unsupported OS: {}", os))),
    };

    ui.status(&format!("Compiling sokol for {} [{}]...", os, arch));

    let default_backend_suffix = backends[0].2;
    let check_lib_name = format!(
        "app/sokol_app_{}_{}_{}_{}.{}",
        os,
        arch,
        default_backend_suffix,
        profile.to_lowercase(),
        if os == "windows" { "lib" } else { "a" }
    );
    let check_path = sokol_dir.join(&check_lib_name);

    if !clean && check_path.exists() {
        if ui.verbose {
            ui.log("Sokol compilation skipped (already compiled).");
        }
        return Ok(());
    }

    if clean {
        ui.status("Cleaning sokol artifacts...");

        for module in SOKOL_MODULES {
            let folder_name = module.strip_prefix("sokol_").unwrap_or(module);
            let target_dir = sokol_dir.join(folder_name);

            clean_dir(&target_dir);
        }
    }

    let mut tasks = Vec::new();
    for module in SOKOL_MODULES {
        for (_, define, suffix) in &backends {
            tasks.push((module, define, suffix));
        }
    }

    tasks
        .par_iter()
        .try_for_each(|(module, define, suffix)| -> Result<(), CustomError> {
            let folder_name = module.strip_prefix("sokol_").unwrap_or(module);

            let output_dir = sokol_dir.join(folder_name);
            fs::create_dir_all(&output_dir).map_err(|e| {
                CustomError::ProcessError(format!(
                    "Failed to create directory {:?}: {}",
                    output_dir, e
                ))
            })?;

            if os == "windows" {
                build_windows(sokol_dir, module, define, suffix, arch, is_debug)?;
            } else {
                build_unix(sokol_dir, module, define, suffix, arch, is_debug)?;
            }
            Ok(())
        })?;

    if os == "windows" {
        if ui.verbose {
            ui.status("Building Windows DLLs...");
        }
    }

    Ok(())
}

fn compile_sokol_wasm(clean: bool, ui: &Ui) -> Result<(), CustomError> {
    ui.status("Compiling sokol (WASM)...");

    let sokol_dir = Path::new(SOKOL_LIB_DIR);

    let (compiler, archiver) = if cfg!(windows) {
        ("emcc.bat", "emar.bat")
    } else {
        ("emcc", "emar")
    };

    let check = Command::new(compiler)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    if check.is_err() {
        return Err(CustomError::BuildError(
            "The 'emcc' command was not found.\n\
            Please install the Emscripten SDK and run 'emsdk_env' to add it to your PATH"
                .to_string(),
        ));
    }

    let modules = &[
        "log",
        "gfx",
        "app",
        "glue",
        "time",
        "audio",
        "debugtext",
        "shape",
        "gl",
    ];

    let profiles = &[
        ("Debug", "debug", "-g"),
        ("Release", "release", "-O2 -DNDEBUG"),
    ];

    if clean {
        ui.status("Cleaning WASM artifacts...");

        for module in modules {
            let target_dir = sokol_dir.join(module);

            clean_dir(&target_dir);
        }
    }

    let mut tasks = Vec::new();
    for module in modules {
        for profile in profiles {
            tasks.push((module, profile));
        }
    }

    tasks.par_iter().try_for_each(
        |(module, (_, prof_suffix, flags))| -> Result<(), CustomError> {
            let src_path = sokol_dir.join(format!("c/sokol_{}.c", module));
            let obj_name = format!("sokol_{}_{}.o", module, prof_suffix);
            let obj_path = sokol_dir.join(&obj_name);
            let out_folder = sokol_dir.join(module);
            let out_lib_name = format!("sokol_{}_wasm_gl_{}.a", module, prof_suffix);
            let out_lib_path = out_folder.join(&out_lib_name);

            fs::create_dir_all(&out_folder).map_err(|e| {
                CustomError::ProcessError(format!(
                    "Failed to create directory {:?}: {}",
                    out_folder, e
                ))
            })?;

            let mut cmd = Command::new(compiler);
            cmd.arg("-c").arg("-DIMPL").arg("-DSOKOL_GLES3");

            for flag in flags.split_whitespace() {
                cmd.arg(flag);
            }

            cmd.arg(&src_path).arg("-o").arg(&obj_path);

            let output = cmd
                .output()
                .map_err(|e| CustomError::ProcessError(format!("Failed to run emcc: {}", e)))?;
            if !output.status.success() {
                return Err(CustomError::BuildError(format!(
                    "WASM compilation failed for {}:\n{}",
                    module,
                    String::from_utf8_lossy(&output.stderr)
                )));
            }

            let mut ar_cmd = Command::new(archiver);
            ar_cmd.arg("rcs").arg(&out_lib_path).arg(&obj_path);

            let output = ar_cmd
                .output()
                .map_err(|e| CustomError::ProcessError(format!("Failed to run emar: {}", e)))?;
            if !output.status.success() {
                return Err(CustomError::BuildError(format!(
                    "WASM archiving failed for {}:\n{}",
                    module,
                    String::from_utf8_lossy(&output.stderr)
                )));
            }

            let _ = fs::remove_file(obj_path);

            Ok(())
        },
    )?;

    Ok(())
}

fn build_windows(
    root_dir: &Path,
    module: &str,
    define: &str,
    suffix: &str,
    arch: &str,
    is_debug: bool,
) -> Result<(), CustomError> {
    let folder = module.strip_prefix("sokol_").unwrap_or(module);
    let profile_suffix = if is_debug { "debug" } else { "release" };

    let src = root_dir.join(format!("c/{}.c", module));
    let obj = root_dir.join(format!(
        "{}_{}_{}_{}.obj",
        module, arch, suffix, profile_suffix
    ));
    let lib = root_dir.join(format!(
        "{}/{}_windows_{}_{}_{}.lib",
        folder, module, arch, suffix, profile_suffix
    ));

    let mut cmd = Command::new("cl");
    cmd.args(&["/c", "/DIMPL", &format!("/D{}", define)]);

    if is_debug {
        cmd.args(&["/D_DEBUG", "/Z7"]);
    } else {
        cmd.args(&["/O2", "/DNDEBUG"]);
    }

    cmd.arg(format!("/Fo{}", obj.display()));
    cmd.arg(&src);

    let output = cmd
        .output()
        .map_err(|e| CustomError::ProcessError(format!("Failed to run cl.exe: {}", e)))?;
    if !output.status.success() {
        return Err(CustomError::BuildError(format!(
            "CL compilation failed for {}:\n{}",
            module,
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    let mut cmd = Command::new("lib");
    cmd.arg(format!("/OUT:{}", lib.display()));
    cmd.arg(&obj);

    let output = cmd
        .output()
        .map_err(|e| CustomError::ProcessError(format!("Failed to run lib.exe: {}", e)))?;
    if !output.status.success() {
        return Err(CustomError::BuildError(format!(
            "LIB failed for {}:\n{}",
            module,
            String::from_utf8_lossy(&output.stdout)
        )));
    }

    let _ = fs::remove_file(obj);
    Ok(())
}

fn build_unix(
    root_dir: &Path,
    module: &str,
    define: &str,
    suffix: &str,
    arch: &str,
    is_debug: bool,
) -> Result<(), CustomError> {
    let folder = module.strip_prefix("sokol_").unwrap_or(module);
    let os = std::env::consts::OS;
    let profile_suffix = if is_debug { "debug" } else { "release" };

    let src = root_dir.join(format!("c/{}.c", module));
    let obj = root_dir.join(format!("{}_{}_{}.o", module, suffix, profile_suffix));
    let lib = root_dir.join(format!(
        "{}/{}_{}_{}_{}_{}.a",
        folder, module, os, arch, suffix, profile_suffix
    ));

    let mut cmd = Command::new("clang");
    cmd.arg("-c");

    if os == "macos" {
        cmd.args(&["-x", "objective-c"]);
        cmd.env("MACOSX_DEPLOYMENT_TARGET", "10.13");

        let mac_arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x86_64",
            _ => {
                return Err(CustomError::BuildError(format!(
                    "unsupported macOS arch: {}",
                    std::env::consts::ARCH
                )));
            }
        };

        cmd.args(&["-arch", mac_arch]);
    } else {
        cmd.args(&["-x", "c", "-fPIC"]);
    }

    cmd.arg("-DIMPL").arg(format!("-D{}", define));

    if is_debug {
        cmd.arg("-g");
    } else {
        cmd.args(&["-O2", "-DNDEBUG"]);
    }

    cmd.arg(&src).arg("-o").arg(&obj);

    let output = cmd
        .output()
        .map_err(|e| CustomError::ProcessError(format!("Clang failed: {}", e)))?;
    if !output.status.success() {
        return Err(CustomError::BuildError(format!(
            "Compilation failed for {}:\n{}",
            module,
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let output = Command::new("ar")
        .args(&["rcs", &lib.to_string_lossy(), &obj.to_string_lossy()])
        .output()
        .map_err(|e| CustomError::ProcessError(format!("Ar failed: {}", e)))?;

    if !output.status.success() {
        return Err(CustomError::BuildError(format!(
            "Archive failed for {}",
            module
        )));
    }

    let _ = fs::remove_file(obj);
    Ok(())
}
