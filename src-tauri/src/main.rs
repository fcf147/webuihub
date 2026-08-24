// 预编译期入口（Windows/Linux/macOS 桌面二进制）。移动端走 lib 的 mobile_entry_point。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    webuihub_lib::run()
}
