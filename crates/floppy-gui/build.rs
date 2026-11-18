fn main() {
    // Release builds ask Windows for Administrator rights at launch (UAC), because
    // reading/writing the raw USB device needs them. Debug/test builds don't, so
    // `cargo test` keeps working from a normal shell.
    let release = std::env::var("PROFILE").as_deref() == Ok("release");
    let mut windows = tauri_build::WindowsAttributes::new();
    if release {
        windows = windows.app_manifest(include_str!("admin.manifest"));
    }
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
}
