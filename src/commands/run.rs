use crate::build::{build_desktop, build_web, clean_build};
use crate::error::CustomError;
use crate::ui::Ui;
use clap::Args;
use colored::Colorize;
use rouille::Server;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

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

    if args.web {
        run_web(args, &ui)?;
    } else {
        run_desktop(args, &ui)?;
    }

    Ok(())
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
