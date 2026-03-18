fn main() {
    // Tell sqlx to use offline mode (no live DB needed at build time)
    println!("cargo:rustc-env=SQLX_OFFLINE=true");
    tauri_build::build()
}
