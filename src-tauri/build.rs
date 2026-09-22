fn main() {
    // Ensure icon / resource changes re-embed into the exe (cargo otherwise
    // may skip winres when only icons/*.png|ico change).
    println!("cargo:rerun-if-changed=icons");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/32x32.png");
    println!("cargo:rerun-if-changed=icons/128x128.png");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    tauri_build::build()
}
