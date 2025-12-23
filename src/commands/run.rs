use crate::build::{build_desktop, build_web, clean_build};
use crate::error::CustomError;
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
    #[arg(long)]
    pub verbose: bool,
}

pub fn run(args: &RunArgs) -> Result<(), CustomError> {
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

    if args.verbose {
        println!(
            "{} Running project in: {}",
            "[INFO]".green(),
            project_dir.display()
        );
    }

    if args.clean {
        clean_build(args.verbose)?;
    }

    if args.web {
        run_web(args)?;
    } else {
        run_desktop(args)?;
    }

    Ok(())
}

fn run_desktop(args: &RunArgs) -> Result<(), CustomError> {
    if args.verbose {
        println!("{} Building for desktop...", "[INFO]".green());
    }

    let build_result = build_desktop(&args.config, false, args.verbose)?;

    if !build_result.executable_path.exists() {
        return Err(CustomError::BuildError(format!(
            "Executable not found at: {}",
            build_result.executable_path.display()
        )));
    }

    println!("{} Running desktop build...", "[INFO]".green());

    let mut child = Command::new(&build_result.executable_path)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| CustomError::ProcessError(format!("Failed to start game: {}", e)))?;

    let status = child
        .wait()
        .map_err(|e| CustomError::ProcessError(format!("Failed to wait for process: {}", e)))?;

    if !status.success() {
        println!(
            "{} Game exited with code: {}",
            "[ERROR]".red().bold(),
            status
        );
    }

    Ok(())
}

fn run_web(args: &RunArgs) -> Result<(), CustomError> {
    if args.verbose {
        println!("{} Building for web...", "[INFO]".green());
    }

    build_web(&args.config, false, args.verbose)?;

    if args.verbose {
        println!("{} Starting web server...", "[INFO]".green());
    }

    let port = args.port;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        let _ = open_browser(port);
    });

    serve_web_directory(Path::new("build/web"), args.port)?;

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

fn serve_web_directory(web_dir: &Path, port: u16) -> Result<(), CustomError> {
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

    ctrlc::set_handler(move || {
        shutdown_clone.store(true, Ordering::SeqCst);
        println!("\n{} Shutting down server...", "[INFO]".green());
    })
    .map_err(|e| CustomError::ProcessError(format!("Failed to set Ctrl+C handler: {}", e)))?;

    println!(
        "{} Serving web build at http://localhost:{}.",
        "[INFO]".green(),
        port
    );
    println!("  (CTRL+C to stop the server)");

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

    println!("{} Server stopped.", "[INFO]".green());
    Ok(())
}
