// GUI subsystem in every build, including debug — otherwise Windows opens a
// console window behind the browser.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    vyro_browser_lib::run();
}
