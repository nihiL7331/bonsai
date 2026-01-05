use crate::Ui;
use crate::error::CustomError;
use anyhow::Result;
use clap::Args;
use regex::Regex;
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
    operator: String, // :: or :
}

pub fn docs(args: &DocsArgs, ui: Ui) -> Result<(), CustomError> {
    let src_dir = args.dir.as_deref().unwrap_or("./bonsai");
    let src_path = Path::new(src_dir);
    let out_dir = args.target.as_deref().unwrap_or("./docs");
    let out_path = Path::new(out_dir);

    ui.status(&format!("Scanning for odin files in: {:?}", src_path));

    let pattern = format!(
        r"(?im)//\s*{}:?\s*((?:.*(?:\n\s*//.*)*))\n(?:.*\n)*?\s*(\w+)\s*(::|:)\s*(?:(proc|struct|enum|union)\s*([\s\S]*?)\{{|([^\n]+))",
        regex::escape(args.trigger.as_str())
    );
    let re = Regex::new(&pattern).map_err(|e| CustomError::ValidationError(e.to_string()))?;

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

                    let items: Vec<DocItem> = re
                        .captures_iter(&content)
                        .map(|cap| {
                            let raw_doc = cap.get(1).map_or("", |m| m.as_str());
                            let name = cap.get(2).map_or("Unknown", |m| m.as_str()).to_string();
                            let operator = cap.get(3).map_or("::", |m| m.as_str()).to_string();
                            let raw_kind_keyword = cap.get(4).map(|m| m.as_str());
                            let header_details = cap.get(5).map_or("", |m| m.as_str());
                            let raw_value_content = cap.get(6).map(|m| m.as_str());
                            let doc = raw_doc
                                .lines()
                                .map(|line| {
                                    let trimmed_line = line.trim();
                                    let content = trimmed_line.trim_start_matches("//");
                                    if let Some(stripped) = content.strip_prefix(' ') {
                                        stripped.trim_end().to_string()
                                    } else {
                                        content.trim_end().to_string()
                                    }
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

                                match k_enum {
                                    ItemKind::Proc => {
                                        if header_details.trim().is_empty() {
                                            let match_whole = cap.get(0).unwrap();
                                            let body_start_index = match_whole.end();

                                            let body_content =
                                                extract_balanced_block(&content, body_start_index)
                                                    .unwrap_or_else(|| "...".to_string());

                                            let clean_body = body_content
                                                .trim_start_matches(|c| c == '\r' || c == '\n')
                                                .lines()
                                                .map(|l| l.strip_prefix("  ").unwrap_or(l))
                                                .collect::<Vec<_>>()
                                                .join("\n");

                                            let sig_string = format!("proc {{\n{}\n}}", clean_body);

                                            (ItemKind::Overload, sig_string)
                                        } else {
                                            (
                                                ItemKind::Proc,
                                                format!("proc {}", header_details.trim()),
                                            )
                                        }
                                    }
                                    ItemKind::Struct | ItemKind::Enum | ItemKind::Union => {
                                        let match_whole = cap.get(0).unwrap();
                                        let body_start_index = match_whole.end();

                                        let body_content =
                                            extract_balanced_block(&content, body_start_index)
                                                .unwrap_or_else(|| {
                                                    "...(error parsing body)".to_string()
                                                });

                                        let clean_body = body_content
                                            .trim_start_matches(|c| c == '\r' || c == '\n')
                                            .lines()
                                            .map(|l| l.strip_prefix("  ").unwrap_or(l))
                                            .collect::<Vec<_>>()
                                            .join("\n");

                                        let sig_string = format!(
                                            "{} {} {{\n{}\n}}",
                                            k,
                                            header_details.trim(),
                                            clean_body
                                        );

                                        (k_enum, sig_string)
                                    }
                                    _ => (ItemKind::Unknown, "unknown".to_string()),
                                }
                            } else if let Some(val) = raw_value_content {
                                let val_clean = val.split("//").next().unwrap_or(val).trim();
                                (ItemKind::Const, val_clean.to_string())
                            } else {
                                (ItemKind::Unknown, "???".to_string())
                            };
                            DocItem {
                                name,
                                doc,
                                kind,
                                signature,
                                operator,
                            }
                        })
                        .collect();

                    if !items.is_empty() {
                        if let Ok(relative_path) = path.strip_prefix(src_path) {
                            let mut out_file_path = out_path.join(relative_path);
                            out_file_path.set_extension("md");

                            if let Some(parent) = out_file_path.parent() {
                                fs::create_dir_all(parent)?;
                            }

                            write_markdown(&items, &out_file_path)?;

                            if ui.verbose {
                                ui.log(&format!("Generated: {:?}", out_file_path));
                            }
                            files_processed += 1;
                        }
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
    ui.success(&format!(
        "Docs generated successfully. Processed {} files.",
        files_processed
    ));

    Ok(())
}

fn write_markdown(items: &[DocItem], path: &PathBuf) -> Result<(), CustomError> {
    let page_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Reference");
    let mut content = format!(
        "---\ntitle: {}\ndescription: Auto-generated API reference (Rust CLI)\n---\n\n",
        page_title
    );

    for item in items {
        content.push_str(&format!("## {}\n\n", item.name));
        let mut private_prefix = "";
        if item.name.starts_with('_') {
            private_prefix = "@(private)";
        }
        content.push_str(&format!(
            "```Odin\n{}\n{} {} {}\n```\n\n",
            private_prefix, item.name, item.operator, item.signature
        ));
        content.push_str(&format!("{}\n\n", item.doc));
        content.push_str("---\n");
    }

    fs::write(path, content)
        .map_err(|_| CustomError::ValidationError(path.display().to_string()))?;

    Ok(())
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
