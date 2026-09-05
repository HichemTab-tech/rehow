fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("failed to build Tauri application");
}
