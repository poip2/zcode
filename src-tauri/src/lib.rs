pub mod agent;
pub mod error;
pub mod model;
pub mod provider;
pub mod sse;
pub mod providers;
pub mod skills;
pub mod tools;
mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            commands::read_markdown_file,
            commands::write_markdown_file,
            commands::resolve_path,
            commands::allow_assets,
            commands::read_dir_tree,
            commands::path_exists,
            commands::create_markdown_file,
            commands::create_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
