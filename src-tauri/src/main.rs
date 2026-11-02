// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        // No argument or launched by login autostart: GUI path, no console.
        None | Some(devku_lib::autostart::HIDDEN_FLAG) => {
            devku_lib::run();
            ExitCode::SUCCESS
        }
        Some("-h" | "--help" | "help") => {
            devku_lib::console::attach_for_cli();
            devku_lib::cli::print_help();
            ExitCode::SUCCESS
        }
        // Otherwise CLI mode: in release builds the parent console is attached first.
        Some(_) => {
            devku_lib::console::attach_for_cli();
            devku_lib::cli::run(&args)
        }
    }
}
