// Không console window khi chạy bản release trên Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    als_desktop_lib::run();
}
