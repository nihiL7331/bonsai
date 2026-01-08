use crate::Ui;
use crate::error::CustomError;
use crate::git::clone_repo_to_temp;
use crate::manifest::update_manifest;
use clap::Args;
use std::fs;
use std::io;
use std::path::Path;
use toml_edit::DocumentMut;
use url::Url;

const SYSTEMS_DIR: &str = "bonsai/systems";
const MANIFEST_FILE: &str = "bonsai.toml";

#[derive(Args)]
pub struct InstallArgs {
    pub url: String,
    #[arg(long, short, default_value = "latest")]
    pub version: String,
    #[arg(long, short)]
    pub name: Option<String>,
}

pub fn install(args: &InstallArgs, ui: Ui) -> Result<(), CustomError> {
    let project_manifest_path = Path::new(MANIFEST_FILE);
    if !project_manifest_path.exists() {
        return Err(CustomError::ValidationError(
            "Bonsai.toml manifest not found. Are you in a bonsai project?".to_string(),
        ));
    }

    let full_url = resolve_url(&args.url);

    let folder_name = match &args.name {
        Some(n) => n.clone(),
        None => extract_name_from_url(&full_url)?,
    };

    if folder_name.contains('/') || folder_name.contains('\\') {
        return Err(CustomError::ValidationError(
            "Invalid system name.".to_string(),
        ));
    }

    let systems_dir = Path::new(SYSTEMS_DIR);
    if !systems_dir.exists() {
        return Err(CustomError::ValidationError(
            "source/systems directory not found. Are you in a bonsai project?".to_string(),
        ));
    }

    let target_path = systems_dir.join(&folder_name);
    if target_path.exists() {
        return Err(CustomError::ValidationError(format!(
            "System '{}' is already installed at {:?}",
            folder_name, target_path
        )));
    }

    ui.status(&format!("Installing system '{}'...", folder_name));

    // 1. clone to temp cache
    let temp_repo = clone_repo_to_temp(&full_url, &args.version, &ui)?;
    let repo_path = temp_repo.path();

    // 2. read manifest
    let manifest_path = repo_path.join("bonsai.toml");
    if !manifest_path.exists() {
        return Err(CustomError::ValidationError(
            "Not a valid Bonsai system".into(),
        ));
    }

    // 3. resolve dependencies recursively
    let manifest_content = fs::read_to_string(&manifest_path)?;
    let doc = manifest_content.parse::<DocumentMut>()?;
    if let Some(deps) = doc.get("dependencies").and_then(|d| d.as_table()) {
        for (dep_name, dep_value) in deps.iter() {
            let ui_clone = ui.clone();
            let dep_url = if let Some(table) = dep_value.as_table() {
                table.get("git").and_then(|v| v.as_str())
            } else {
                None
            };

            if let Some(url) = dep_url {
                ui_clone.status(&format!("Resolving dependency '{}'...", dep_name));
                let dep_args = InstallArgs {
                    url: url.to_string(),
                    name: Some(dep_name.to_string()),
                    version: "latest".to_string(),
                };
                install(&dep_args, ui_clone)?;
            }
        }
    }

    // 4. copy system to target dir
    let source_system_path = repo_path.join("bonsai/systems").join(&folder_name);

    if !source_system_path.exists() {
        return Err(CustomError::ValidationError(format!(
            "The repository does not contain 'bonsai/systems/{}'. Structure mismatch.",
            folder_name
        )));
    }

    ui.status("Copying system files...");

    copy_dir_all(&source_system_path, &target_path).map_err(|e| CustomError::IoError(e))?;

    // 5. install utils (optional)
    let source_utils_path = repo_path.join("utils");
    if source_utils_path.exists() {
        let project_utils_dir = Path::new("utils");
        if !project_utils_dir.exists() {
            fs::create_dir_all(project_utils_dir).map_err(|e| CustomError::IoError(e))?;
        }

        let target_utils_path = project_utils_dir.join(&folder_name);

        ui.status(&format!(
            "Found utilities. Installing to 'utils/{}'",
            folder_name
        ));

        copy_dir_all(&source_utils_path, &target_utils_path)
            .map_err(|e| CustomError::IoError(e))?;
    }

    ui.status("Updating manifest...");
    update_manifest(Path::new("."), &ui)?;

    ui.success(&format!("Installed {} successfully.", folder_name));
    Ok(())
}

fn extract_name_from_url(url_str: &str) -> Result<String, CustomError> {
    if url_str.starts_with("git@") {
        let last_segment =
            url_str
                .rsplit(|c| c == '/' || c == ':')
                .next()
                .ok_or(CustomError::ValidationError(
                    "Invalid git SSH format".into(),
                ))?;

        if last_segment == url_str {
            return Err(CustomError::ValidationError(
                "Git SSH URL must contain a separator (: or /)".into(),
            ));
        }

        let name = last_segment.trim_end_matches(".git");
        if name.is_empty() {
            return Err(CustomError::ValidationError(
                "Could not determine system name".into(),
            ));
        }

        return Ok(name.to_string());
    }

    let url = Url::parse(url_str)
        .map_err(|e| CustomError::ValidationError(format!("Invalid URL: {}", e)))?;

    let segments = url
        .path_segments()
        .ok_or(CustomError::ValidationError("URL has no path".into()))?;
    let last = segments
        .filter(|s| !s.is_empty())
        .last()
        .ok_or(CustomError::ValidationError("URL has no segments".into()))?;

    let name = if last.ends_with(".git") {
        last.trim_end_matches(".git")
    } else {
        last
    };

    if name.is_empty() {
        return Err(CustomError::ValidationError(
            "Could not determine system name from URL".into(),
        ));
    }

    Ok(name.to_string())
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn resolve_url(input: &str) -> String {
    if input.starts_with("http") || input.starts_with("git@") {
        input.to_string()
    } else {
        format!("https://github.com/{}.git", input)
    }
}
