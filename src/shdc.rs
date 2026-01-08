use crate::Ui;
use crate::error::CustomError;
use std::{env, fs, fs::File, io, path::PathBuf};

const SHDC_BASE_URL: &str = "https://raw.githubusercontent.com/floooh/sokol-tools-bin/master/bin";
const BONSAI_BIN_DIR: &str = ".bonsai/bin";

fn get_executable_name() -> &'static str {
    if env::consts::OS == "windows" {
        "sokol-shdc.exe"
    } else {
        "sokol-shdc"
    }
}

pub fn get_shdc_url() -> Result<String, CustomError> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    match (os, arch) {
        ("windows", _) => Ok(format!("{}/win32/sokol-shdc.exe", SHDC_BASE_URL)),
        ("linux", _) => Ok(format!("{}/linux/sokol-shdc", SHDC_BASE_URL)),
        ("macos", "aarch64") => Ok(format!("{}/osx_arm64/sokol-shdc", SHDC_BASE_URL)),
        ("macos", _) => Ok(format!("{}/osx/sokol-shdc", SHDC_BASE_URL)),
        _ => Err(CustomError::ValidationError(format!(
            "Unsupported platform: {} {}",
            os, arch
        ))),
    }
}

fn install_shdc(ui: &Ui) -> Result<PathBuf, CustomError> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| CustomError::ValidationError("Could not find home directory".into()))?;

    let install_dir = home_dir.join(BONSAI_BIN_DIR);

    fs::create_dir_all(&install_dir)?;

    let dest_path = install_dir.join(get_executable_name());
    let url = get_shdc_url()?;

    ui.message(&format!(
        "Downloading sokol-shdc for {}...",
        env::consts::OS
    ));
    ui.message(&format!("  Source: {}", url));

    let response = ureq::get(&url)
        .call()
        .map_err(|e| CustomError::BuildError(format!("Failed to download sokol-shdc: {}", e)))?;

    let mut dest_file = File::create(&dest_path).map_err(|e| CustomError::IoError(e))?;

    let mut reader = response.into_body().into_reader();

    io::copy(&mut reader, &mut dest_file).map_err(|e| CustomError::IoError(e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest_path, perms)?;
    }

    ui.log(&format!("Installed sokol-shdc to {:?}", dest_path));
    Ok(dest_path)
}

pub fn get_or_install_shdc(ui: &Ui) -> PathBuf {
    let home = dirs::home_dir().expect("Home directory required.");
    let path = home.join(BONSAI_BIN_DIR).join(get_executable_name());

    if path.exists() {
        return path;
    }

    match install_shdc(ui) {
        Ok(p) => p,
        Err(_) => {
            ui.error("Failed to install sokol-shdc. Please check your internet connection.");
            std::process::exit(1);
        }
    }
}
