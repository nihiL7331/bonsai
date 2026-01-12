use crate::Ui;
use crate::error::CustomError;
use std::{env, fs, fs::File, io, path::PathBuf};
use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig};

const SHDC_BASE_URL: &str = "https://raw.githubusercontent.com/floooh/sokol-tools-bin/master/bin";

fn get_install_dir() -> Result<PathBuf, CustomError> {
    let base_dir = dirs::data_local_dir().ok_or_else(|| {
        CustomError::ValidationError("Could not find local data directory".into())
    })?;

    let install_dir = base_dir.join("bonsai").join("bin");

    Ok(install_dir)
}

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
    let install_dir = get_install_dir()?;
    fs::create_dir_all(&install_dir).map_err(|e| {
        CustomError::IoError(std::io::Error::new(
            e.kind(),
            format!("Failed to create directory {:?}: {}", install_dir, e),
        ))
    })?;

    let dest_path = install_dir.join(get_executable_name());
    let url = get_shdc_url()?;

    ui.message(&format!(
        "Downloading sokol-shdc for {}...",
        env::consts::OS
    ));
    ui.message(&format!("  Source: {}", url));

    let agent = Agent::config_builder()
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent();

    let response = agent
        .get(&url)
        .header("User-Agent", "bonsai-cli")
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
    let install_dir = match get_install_dir() {
        Ok(d) => d,
        Err(e) => {
            ui.error(&format!("Critical error resolving paths: {}", e));
            std::process::exit(1);
        }
    };

    let path = install_dir.join(get_executable_name());

    if path.exists() {
        return path;
    }

    match install_shdc(ui) {
        Ok(p) => p,
        Err(e) => {
            ui.error(&format!("Failed to install sokol-shdc: {}", e));

            ui.error("Make sure you have internet access and file write permissions.");
            std::process::exit(1);
        }
    }
}
