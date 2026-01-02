use chrono::Local;
use colored::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::io;
use std::io::Write;

#[derive(Clone)]
pub struct Ui {
    spinner: ProgressBar,
    verbose: bool,
    multiprogress: MultiProgress,
}

impl Ui {
    pub fn new(verbose: bool) -> Self {
        let pb = if verbose {
            ProgressBar::hidden()
        } else {
            let p = ProgressBar::new_spinner();
            p.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
            );
            p
        };

        Self {
            spinner: pb,
            multiprogress: MultiProgress::new(),
            verbose,
        }
    }

    fn timestamp(&self) -> String {
        let now = Local::now();
        format!("{}", now.format("[%H:%M:%S]").to_string().bright_yellow())
    }

    pub fn status(&self, msg: &str) {
        let time = self.timestamp();
        if self.verbose {
            println!("{} {} {}", time, "[INFO]".blue().bold(), msg);
        } else {
            self.spinner
                .set_message(format!("{} {}", time, msg.to_string()));
        }
    }

    pub fn log(&self, msg: &str) {
        let time = self.timestamp();
        if self.verbose {
            println!("{} {} {}", time, "[LOG]".yellow().bold(), msg);
        } else {
            let _ =
                self.multiprogress
                    .println(format!("{} {} {}", time, "[LOG]".yellow().bold(), msg));
        }
    }

    pub fn success(&self, msg: &str) {
        let time = self.timestamp();
        if self.verbose {
            println!("{} {} {}", time, "[SUCCESS]".green(), msg);
        } else {
            self.spinner
                .finish_with_message(format!("{} {} {}", time, "✔".green(), msg));
        }
    }

    pub fn error(&self, msg: &str) {
        let time = self.timestamp();
        if self.verbose {
            eprintln!("{} {} {}", time, "[ERROR]".red().bold(), msg);
        } else {
            self.spinner
                .finish_with_message(format!("{} {} {}", time, "✘".red(), msg));
        }
    }

    pub fn message(&self, text: &str) {
        let time = self.timestamp();
        if self.verbose {
            println!("{} {}", time, text);
        } else {
            let _ = self.multiprogress.println(format!("{} {}", time, text));
        }
    }

    pub fn confirm(&self, prompt_text: &str) -> bool {
        let time = self.timestamp();
        self.multiprogress.suspend(|| {
            print!("{} {} [Y/N]: ", time, prompt_text);
            io::stdout().flush().unwrap_or(());
            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap_or(0);
            let clean = input.trim().to_lowercase();
            clean == "y" || clean == "yes"
        })
    }

    pub fn create_bar(&self) -> ProgressBar {
        let pb = self.multiprogress.add(ProgressBar::new(0));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len}")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb
    }
}
