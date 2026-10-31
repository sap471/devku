// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        // Tanpa argumen atau dari autostart login: jalur GUI, tanpa konsol.
        None | Some(devku_lib::autostart::HIDDEN_FLAG) => {
            devku_lib::run();
            ExitCode::SUCCESS
        }
        Some("-h" | "--help" | "help") => {
            devku_lib::cli::print_help();
            ExitCode::SUCCESS
        }
        // Selain itu mode CLI: di build release konsol induk dipasang dulu.
        Some(_) => {
            devku_lib::console::attach_for_cli();
            devku_lib::cli::run(&args)
        }
    }
}
