use crate::Ui;
use crate::error::CustomError;
use clap::Args;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Args)]
pub struct DocsArgs {
    #[arg(long, short)]
    pub dir: Option<String>,
    #[arg(long, short)]
    pub target: Option<String>,
    pub trigger: String,
}

#[derive(Debug, Clone, PartialEq)]
enum ItemKind {
    Proc,
    Overload,
    Struct,
    Enum,
    Union,
    Const,
    Unknown,
}

#[derive(Debug)]
struct DocItem {
    name: String,
    doc: String,
    #[allow(dead_code)]
    kind: ItemKind,
    signature: String,
    operator: String,
}

struct PackageData {
    overview: String,
    items: Vec<DocItem>,
}

pub fn docs(args: &DocsArgs, ui: Ui) -> Result<(), CustomError> {
    let src_dir = args.dir.as_deref().unwrap_or("./bonsai");
    let src_path = Path::new(src_dir);
    let out_dir = args.target.as_deref().unwrap_or("./docs");
    let out_path = Path::new(out_dir);

    ui.status(&format!("Scanning for odin files in: {:?}", src_path));

    let pattern = format!(
        r"(?im)//\s*{}:?\s*((?:.*(?:\n\s*//.*)*))\n(?:.*\n)*?\s*(\w+)\s*(::|:)\s*(?:(proc|struct|enum|union)|([^\n]+))",
        regex::escape(args.trigger.as_str())
    );
    let re = Regex::new(&pattern).map_err(|e| CustomError::ValidationError(e.to_string()))?;

    let overview_re = Regex::new(r"(?im)//\s*@overview\s*:?\s*((?:.*(?:\n\s*//.*)*))").unwrap();

    let mut package_map: HashMap<PathBuf, PackageData> = HashMap::new();
    let mut files_processed = 0;

    for entry in WalkDir::new(src_path) {
        match entry {
            Ok(entry) => {
                let path = entry.path();

                if path.extension().map_or(false, |ext| ext == "odin") {
                    let content = fs::read_to_string(path).map_err(|_| {
                        CustomError::ValidationError(format!(
                            "Failed to read file: {}",
                            path.display()
                        ))
                    })?;

                    let mut file_items = Vec::new();
                    let mut file_overview = String::new();

                    if let Some(cap) = overview_re.captures(&content) {
                        let raw_overview = cap.get(1).map_or("", |m| m.as_str());
                        file_overview = raw_overview
                            .lines()
                            .map(|line| {
                                let content = line.trim().trim_start_matches("//");
                                content
                                    .strip_prefix(' ')
                                    .unwrap_or(content)
                                    .trim_end()
                                    .to_string()
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                    }

                    for cap in re.captures_iter(&content) {
                        let raw_doc = cap.get(1).map_or("", |m| m.as_str());
                        let name = cap.get(2).map_or("Unknown", |m| m.as_str()).to_string();
                        let operator = cap.get(3).map_or("::", |m| m.as_str()).to_string();
                        let raw_kind_keyword = cap.get(4).map(|m| m.as_str());
                        let raw_value_content = cap.get(5).map(|m| m.as_str());

                        let doc = raw_doc
                            .lines()
                            .map(|line| {
                                let trimmed = line.trim();
                                let content = trimmed.trim_start_matches("//");
                                content
                                    .strip_prefix(' ')
                                    .unwrap_or(content)
                                    .trim_end()
                                    .to_string()
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        let (kind, signature) = if let Some(k) = raw_kind_keyword {
                            let k_enum = match k {
                                "proc" => ItemKind::Proc,
                                "struct" => ItemKind::Struct,
                                "enum" => ItemKind::Enum,
                                "union" => ItemKind::Union,
                                _ => ItemKind::Unknown,
                            };
                            let kind_match = cap.get(4).unwrap();
                            let scan_start = kind_match.end();
                            let (raw_header, body_start_index) = scan_header(&content, scan_start);
                            let header_clean = raw_header.trim();

                            match k_enum {
                                ItemKind::Proc => {
                                    let is_overload = header_clean.is_empty();
                                    if is_overload {
                                        let sig_string = if body_start_index > 0 {
                                            let body =
                                                extract_balanced_block(&content, body_start_index)
                                                    .unwrap_or("...".into());
                                            let clean = clean_body_indentation(body);
                                            format!("proc {{\n{}\n}}", clean)
                                        } else {
                                            "proc { ... }".into()
                                        };
                                        (ItemKind::Overload, sig_string)
                                    } else {
                                        (ItemKind::Proc, format!("proc {}", header_clean))
                                    }
                                }
                                ItemKind::Struct | ItemKind::Enum | ItemKind::Union => {
                                    let sig_string = if body_start_index > 0 {
                                        let body =
                                            extract_balanced_block(&content, body_start_index)
                                                .unwrap_or("...".into());
                                        let clean = clean_body_indentation(body);
                                        format!("{} {} {{\n{}\n}}", k, header_clean, clean)
                                    } else {
                                        format!("{} {}", k, header_clean)
                                    };
                                    (k_enum, sig_string)
                                }
                                _ => (ItemKind::Unknown, "unknown".to_string()),
                            }
                        } else if let Some(val) = raw_value_content {
                            if let Some(brace_offset) = val.find('{') {
                                let val_match = cap.get(5).unwrap();
                                let body_start = val_match.start() + brace_offset + 1;

                                let sig_string = if let Some(block) =
                                    extract_balanced_block(&content, body_start)
                                {
                                    let clean = clean_body_indentation(block);
                                    let prefix = val[..brace_offset].trim();
                                    format!("{} {{\n{}\n}}", prefix, clean)
                                } else {
                                    val.trim().to_string()
                                };
                                (ItemKind::Const, sig_string)
                            } else {
                                let val_clean = val.split("//").next().unwrap_or(val).trim();
                                (ItemKind::Const, val_clean.to_string())
                            }
                        } else {
                            (ItemKind::Unknown, "???".to_string())
                        };

                        let item = DocItem {
                            name,
                            doc,
                            kind,
                            signature,
                            operator,
                        };

                        file_items.push(item);
                    }

                    if !file_items.is_empty() || !file_overview.is_empty() {
                        let parent_dir = path.parent().unwrap_or(src_path);
                        let relative_package_path = parent_dir
                            .strip_prefix(src_path)
                            .unwrap_or(Path::new(""))
                            .to_path_buf();

                        let package_data =
                            package_map
                                .entry(relative_package_path)
                                .or_insert(PackageData {
                                    overview: String::new(),
                                    items: Vec::new(),
                                });

                        if !file_overview.is_empty() {
                            if !package_data.overview.is_empty() {
                                package_data.overview.push_str("\n\n");
                            }
                            package_data.overview.push_str(&file_overview);
                        }

                        package_data.items.extend(file_items);
                        files_processed += 1;
                    }
                }
            }
            Err(e) => {
                if ui.verbose {
                    ui.log(&format!("Skipping entry: {}", e))
                }
            }
        }
    }

    ui.status("Writing package pages...");

    for (rel_path, mut data) in package_map {
        // Sort items alphabetically so the page is readable
        data.items.sort_by(|a, b| a.name.cmp(&b.name));

        let file_stem = if rel_path.components().count() == 0 {
            Path::new("index")
        } else {
            &rel_path
        };

        let out_file_path = out_path.join(file_stem).with_extension("md");

        if let Some(parent) = out_file_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let title_str = rel_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Root")
            .to_string();

        write_package_markdown(&title_str, &data.overview, &data.items, &out_file_path)?;
        if ui.verbose {
            ui.log(&format!("Generated Package: {:?}", out_file_path));
        }
    }

    ui.success(&format!(
        "Docs generated successfully. Scanned {} files, created {} pages.",
        files_processed,
        out_path.read_dir()?.count()
    ));
    Ok(())
}

// Updated Writer Function
fn write_package_markdown(
    title: &str,
    overview: &str,
    items: &[DocItem],
    path: &PathBuf,
) -> Result<(), CustomError> {
    let mut content = format!(
        "---\ntitle: {}\ndescription: API Reference for {} package\n---\n\n",
        title, title
    );

    if !overview.is_empty() {
        content.push_str("## Overview\n\n");
        content.push_str(overview);
        content.push_str("\n\n---\n\n");
    }

    for item in items {
        content.push_str(&format!("## {}\n\n", item.name));

        let private_prefix = if item.name.starts_with('_') {
            "@(private)\n"
        } else {
            ""
        };

        let clean_operator = if item.operator.eq("::") { " ::" } else { ":" };

        content.push_str(&format!(
            "```Odin\n{}{}{} {}\n```\n\n",
            private_prefix, item.name, clean_operator, item.signature
        ));

        content.push_str(&format!("{}\n\n", item.doc));
        content.push_str("---\n");
    }

    fs::write(path, content)
        .map_err(|_| CustomError::ValidationError(path.display().to_string()))?;

    Ok(())
}

// Helpers (Same as before, just separated for cleanliness)
fn clean_body_indentation(body: String) -> String {
    body.trim_start_matches(|c| c == '\r' || c == '\n')
        .lines()
        .map(|l| l.strip_prefix("    ").unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_balanced_block(content: &str, start_index: usize) -> Option<String> {
    let mut depth = 1;
    let mut end_index = start_index;
    for (i, char) in content[start_index..].char_indices() {
        match char {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end_index = start_index + i;
                    break;
                }
            }
            _ => {}
        }
    }
    if depth == 0 {
        Some(content[start_index..end_index].to_string())
    } else {
        None
    }
}

fn scan_header(content: &str, start_index: usize) -> (String, usize) {
    let mut paren_depth = 0;
    let mut header = String::new();
    #[allow(unused_assignments)]
    let mut body_start_index = 0;
    for (i, c) in content[start_index..].char_indices() {
        match c {
            '(' => paren_depth += 1,
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1
                }
            }
            '{' => {
                if paren_depth == 0 {
                    body_start_index = start_index + i + 1;
                    return (header, body_start_index);
                }
            }
            _ => {}
        }
        header.push(c);
    }
    (header, 0)
}
