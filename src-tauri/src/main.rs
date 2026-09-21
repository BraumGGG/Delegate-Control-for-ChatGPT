#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    delegate_control_lib::run();
}
