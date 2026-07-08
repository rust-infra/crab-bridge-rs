#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(err) = crabbridge_desktop::run() {
        eprintln!("crabbridge-desktop: {err:#}");
        std::process::exit(1);
    }
}
