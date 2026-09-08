fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "choose_file",
            "read_document",
            "release_document",
            "watch_document",
            "unwatch_document",
            "follow_link",
            "copy_text",
            "take_open_requests",
        ]),
    ))
    .expect("Tauri build configuration");
}
