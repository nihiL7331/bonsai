use crate::build::{build_desktop, build_web, clean_build};
use crate::error::CustomError;
use crate::ui::Ui;
use clap::Args;
use colored::Colorize;
use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use rouille::Server;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Instant, Duration};
use crate::packer::pack_atlas;

const ASSETS_IMAGES_DIR: &str = "assets";
const ATLAS_DIR: &str = "bonsai/core/render/atlas";

#[derive(Args)]
pub struct RunArgs {
    #[arg(default_value = ".")]
    pub dir: String,
    #[arg(long, conflicts_with = "web")]
    pub desktop: bool,
    #[arg(long, conflicts_with = "desktop")]
    pub web: bool,
    #[arg(long, short = 'c', default_value = "debug")]
    pub config: String,
    #[arg(long)]
    pub clean: bool,
    #[arg(long, short = 'p', default_value_t = 8080)]
    pub port: u16,
}

pub fn run(args: &RunArgs, ui: Ui) -> Result<(), CustomError> {
    let project_dir = Path::new(&args.dir);
    if !project_dir.exists() {
        return Err(CustomError::ValidationError(format!(
            "Directory '{}' does not exist",
            args.dir
        )));
    }

    let manifest_path = project_dir.join("bonsai.toml");
    if !manifest_path.exists() {
        return Err(CustomError::ValidationError(format!(
            "Not a bonsai project directory: '{}'. (Missing bonsai.toml)",
            args.dir
        )));
    }

    let current_dir = std::env::current_dir()?;
    std::env::set_current_dir(project_dir)?;

    let _cleanup_on_fail = scopeguard::guard(current_dir, |dir| {
        let _ = std::env::set_current_dir(&dir);
    });

    ui.status(&format!("Running project in: {}...", project_dir.display()));

    if args.clean {
        clean_build(&ui)?;
    }

    let ws_port = args.port + 1;
    let watch_dir = project_dir.join(ASSETS_IMAGES_DIR);
    spawn_hot_reloader(&ui, ws_port, watch_dir, args.web);

    if args.web {
        run_web(args, &ui)?;
    } else {
        run_desktop(args, &ui)?;
    }

    Ok(())
}

fn spawn_hot_reloader(ui: &Ui, ws_port: u16, target_dir: PathBuf, is_web: bool) {
    if !target_dir.exists() {
        ui.error(&format!("Watch directory missing: {}", target_dir.display()));
        return;
    }

    let ui_clone = ui.clone();
    let ui_ws_clone = ui.clone();

    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    thread::spawn(move || {
        let (debounce_tx, debounce_rx) = mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_millis(200), debounce_tx)
            .expect("Failed to create file watcher");

        debouncer
            .watcher()
            .watch(&target_dir, RecursiveMode::Recursive)
            .expect("Failed to watch target directory");

        ui_clone.status("Hot reloader waiting for asset changes...");

        let mut last_repack_time = Instant::now() - Duration::from_secs(10);
        let cooldown_duration = Duration::from_millis(1000);

        for result in debounce_rx {
            if let Ok(events) = result {
                let mut should_repack = false;
                for event in events {
                    if let Some(ext) = event.path.extension() {
                        if ext == "png" {
                            should_repack = true;
                        }
                    }
                }

                if should_repack {
                    if last_repack_time.elapsed() < cooldown_duration {
                        continue;
                    }

                    last_repack_time = Instant::now();

                    ui_clone.status("Repacking atlas...");

                    let atlas_output_dir = Path::new(ATLAS_DIR);

                    match pack_atlas(&target_dir, &atlas_output_dir, &ui_clone) {
                        Ok(Some(payload)) => {
                            let mut ws_binary = Vec::new();

                            let bin_len = payload.metadata_bin.len() as u32;

                            ws_binary.extend_from_slice(&bin_len.to_le_bytes());
                            ws_binary.extend_from_slice(&payload.metadata_bin);
                            ws_binary.extend_from_slice(&payload.png_bytes);

                            let _ = tx.send(ws_binary);
                        }
                        Ok(None) => {}
                        Err(e) => ui_clone.error(&format!("Packer failed: {}", e)),
                    }
                }
            }
        }
    });

    if is_web {
        thread::spawn(move || {
            let addr = format!("0.0.0.0:{}", ws_port);
            let server = TcpListener::bind(&addr).expect("Failed to bind WS port");

            server.set_nonblocking(true).expect("Cannot set non-blocking");

            ui_ws_clone.message(&format!(
                "{} WebSocket hot-reload server running at ws://localhost:{}",
                "[INFO]".green(),
                ws_port,
            ));

            let mut clients = Vec::new();

            loop {
                if let Ok((stream, _)) = server.accept() {
                    stream.set_nonblocking(false).unwrap();
                    if let Ok(ws) = tungstenite::accept(stream) {
                        clients.push(ws);
                    }
                }

                if let Ok(payload) = rx.try_recv() {
                    clients.retain_mut(|client| {
                        let msg = tungstenite::Message::Binary(payload.clone().into());
                        client.send(msg).is_ok()
                    });
                }

                thread::sleep(Duration::from_millis(16));
            }
        });
    }
}

fn run_desktop(args: &RunArgs, ui: &Ui) -> Result<(), CustomError> {
    ui.status("Building for desktop...");

    let build_result = build_desktop(&args.config, false, ui)?;

    if !build_result.executable_path.exists() {
        return Err(CustomError::BuildError(format!(
            "Executable not found at: {}",
            build_result.executable_path.display()
        )));
    }

    ui.success("Running desktop build...");
    println!("");

    let mut child = Command::new(&build_result.executable_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| CustomError::ProcessError(format!("Failed to start game: {}", e)))?;

    let status = child
        .wait()
        .map_err(|e| CustomError::ProcessError(format!("Failed to wait for process: {}", e)))?;

    if !status.success() {
        ui.error(&format!("Game exited with code: {}", status));
    }

    Ok(())
}

fn run_web(args: &RunArgs, ui: &Ui) -> Result<(), CustomError> {
    ui.status("Building for web...");

    build_web(&args.config, false, ui)?;

    ui.status("Starting web server...");

    let port = args.port;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        let _ = open_browser(port);
    });

    serve_web_directory(Path::new("build/web"), args.port, ui)?;

    Ok(())
}

fn open_browser(port: u16) -> Result<(), CustomError> {
    let url = format!("http://localhost:{}", port);

    #[cfg(target_os = "windows")]
    Command::new("cmd").args(["/C", "start", &url]).spawn()?;

    #[cfg(target_os = "macos")]
    Command::new("open").arg(&url).spawn()?;

    #[cfg(target_os = "linux")]
    Command::new("xdg-open").arg(&url).spawn()?;

    // std::thread::sleep(std::time::Duration::from_secs(1));

    Ok(())
}

fn serve_web_directory(web_dir: &Path, port: u16, ui: &Ui) -> Result<(), CustomError> {
    if !web_dir.exists() {
        return Err(CustomError::ValidationError(format!(
            "Web build does not exist: {}",
            web_dir.display()
        )));
    }

    let addr = format!("0.0.0.0:{}", port);
    let root = web_dir.to_path_buf();

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    let ui_clone = ui.clone();

    ctrlc::set_handler(move || {
        shutdown_clone.store(true, Ordering::SeqCst);
        ui_clone.error("Received Ctrl+C. Shutting down server...");
    })
    .map_err(|e| CustomError::ProcessError(format!("Failed to set Ctrl+C handler: {}", e)))?;

    ui.message(&format!(
        "{} Serving web build at http://localhost:{}.",
        "[INFO]".green(),
        port
    ));
    ui.message("  (CTRL+C to stop the server)");

    ui.status("Server running...");

    let server = Server::new(&addr, move |request| {
        let url = request.url();
        let mut response = rouille::match_assets(request, &root);

        if !response.is_success() && url == "/" {
            let index_path = root.join("index.html");
            if let Ok(file) = std::fs::File::open(index_path) {
                response = rouille::Response::from_file("text/html", file);
            }
        }

        if response.is_success() {
            response = response
                .with_additional_header("Cross-Origin-Opener-Policy", "same-origin")
                .with_additional_header("Cross-Origin-Embedder-Policy", "require-corp")
                .with_additional_header("Cache-Control", "no-cache, no-store, must-revalidate");
        }

        response
    })
    .map_err(|e| CustomError::ProcessError(format!("Failed to start server: {}", e)))?;

    while !shutdown.load(Ordering::SeqCst) {
        server.poll();
        thread::sleep(std::time::Duration::from_millis(50));
    }

    Ok(())
}
