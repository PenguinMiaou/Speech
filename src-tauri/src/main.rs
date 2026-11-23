// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--mcp` serves the meetings to an AI tool over standard input and output, before the
    // app starts, so no window opens and a running app is not asked to handle it.
    if opentypeless_lib::mcp::wanted(std::env::args()) {
        std::process::exit(opentypeless_lib::mcp::serve());
    }
    opentypeless_lib::run()
}
