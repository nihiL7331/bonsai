use crate::Ui;
use crate::error::CustomError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, InlineTable, Value, table, value};

const MANIFEST_FILE: &str = "bonsai.toml";
const SYSTEM_MANIFEST: &str = "system.toml";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum System {
    Version(String),
    Path { path: String },
    Git { url: String, tag: Option<String> },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub project: ProjectInfo,
    #[serde(default)]
    pub build: BuildOptions,
    #[serde(default)]
    pub systems: BTreeMap<String, System>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct BuildOptions {
    #[serde(default)]
    pub web_libs: Vec<String>,
}

pub fn update_manifest(project_root: &Path, ui: &Ui) -> Result<(), CustomError> {
    let manifest_path = project_root.join(MANIFEST_FILE);
    let systems_path = project_root.join("bonsai/systems");

    if !systems_path.exists() {
        return Ok(());
    }

    if !manifest_path.exists() {
        return Err(CustomError::ValidationError(format!(
            "Bonsai manifest not found at {:?}",
            manifest_path
        )));
    }

    let manifest_content = fs::read_to_string(&manifest_path)?;
    let mut doc = manifest_content.parse::<DocumentMut>()?;

    if doc.get("systems").is_none() {
        doc["systems"] = table();
    }

    let deps = doc["systems"].as_table_mut().ok_or_else(|| {
        CustomError::ValidationError("Manifest [systems] is not a table.".to_string())
    })?;

    for entry in fs::read_dir(&systems_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let system_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };

            let sys_toml_path = path.join(SYSTEM_MANIFEST);
            if !sys_toml_path.exists() {
                ui.message(&format!(
                    "  + Auto-generating manifest for system: '{}'",
                    system_name
                ));
                create_default_system_toml(&sys_toml_path, system_name)?;
            }

            if !deps.contains_key(system_name) {
                ui.message(&format!(
                    "  + Discovered new local system: '{}'",
                    system_name
                ));

                let mut t = InlineTable::new();
                t.insert(
                    "path",
                    Value::from(format!("bonsai/systems/{}", system_name)),
                );
                deps.insert(system_name, value(t));
            }
        }
    }
    let mut to_remove = Vec::new();
    let project_root_abs = Path::new(project_root)
        .canonicalize()
        .unwrap_or(Path::new(project_root).to_path_buf());
    let systems_root_abs = &systems_path.canonicalize().unwrap_or(systems_path.clone());

    for (name, item) in deps.iter() {
        if let Some(inline_table) = item.as_inline_table() {
            if let Some(path_val) = inline_table.get("path") {
                if let Some(path_str) = path_val.as_str() {
                    let full_path = project_root_abs.join(path_str);

                    if !full_path.exists() {
                        ui.message(&format!("  - Pruning missing system: '{}'", name));
                        to_remove.push(name.to_string());
                        continue;
                    }

                    match is_path_safe(&systems_root_abs, &full_path) {
                        Ok(false) => {
                            ui.message(&format!(
                                "  - Removing unsafe system path (outside source): '{}'",
                                name
                            ));
                            to_remove.push(name.to_string());
                        }
                        Err(e) => {
                            ui.message(&format!(
                                "  ! Error verifying system path '{}': {}",
                                name, e
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    for name in to_remove {
        deps.remove(&name);
    }

    fs::write(&manifest_path, doc.to_string())?;

    Ok(())
}

fn create_default_system_toml(path: &Path, name: &str) -> Result<(), CustomError> {
    let template = format!(
        r#"[system]
name = {}
version = {}
description = "Auto-generated description for {}"

[dependencies]
# Add system dependencies here
"#,
        name,
        env!("CARGO_PKG_VERSION"),
        name
    );

    fs::write(path, template)?;
    Ok(())
}

pub fn create_manifest(destination: &Path, project_name: &str) -> Result<(), CustomError> {
    let manifest = Manifest {
        project: ProjectInfo {
            name: project_name.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        build: BuildOptions { web_libs: vec![] },
        systems: BTreeMap::new(),
    };

    let manifest_path = destination.join(MANIFEST_FILE);
    let toml_content = toml_edit::ser::to_string_pretty(&manifest).map_err(|e| {
        CustomError::ValidationError(format!("Failed to serialize manifest: {}", e))
    })?;

    fs::write(manifest_path, toml_content)?;

    Ok(())
}

fn is_path_safe(root: &Path, child: &Path) -> Result<bool, CustomError> {
    let root_abs = root
        .canonicalize()
        .map_err(|e| CustomError::ValidationError(format!("Root not found: {}", e)))?;

    let child_abs = child
        .canonicalize()
        .map_err(|e| CustomError::ValidationError(format!("System path not found: {}", e)))?;

    Ok(child_abs.starts_with(&root_abs))
}
