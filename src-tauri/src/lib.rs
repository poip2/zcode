pub mod agent;
mod commands;
pub mod error;
pub mod model;
pub mod provider;
pub mod providers;
pub mod settings;
pub mod skills;
pub mod sse;
pub mod tools;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            // Migrate legacy cleartext apiKey from zcode-settings.json to keychain
            if let Ok(config_dir) = app.path().app_config_dir() {
                crate::settings::migrate_old_settings(&config_dir);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::read_markdown_file,
            commands::write_markdown_file,
            commands::resolve_path,
            commands::allow_assets,
            commands::read_dir_tree,
            commands::path_exists,
            commands::create_markdown_file,
            commands::create_folder,
            commands::save_api_key,
            commands::call_ai_provider,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
