pub mod agent;
pub mod agent_command;
mod commands;
pub mod compaction;
pub mod error;
pub mod model;
pub mod provider;
pub mod providers;
pub mod runtime_env;
pub mod settings;
pub mod skills;
pub mod sse;
pub mod tools;
pub mod watcher;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .setup(|app| {
            // Migrate legacy cleartext apiKey from zcode-settings.json to keychain.
            // tauri-plugin-store may persist to app_data_dir or app_config_dir,
            // so check both.
            if let Ok(data_dir) = app.path().app_data_dir() {
                if let Ok(config_dir) = app.path().app_config_dir() {
                    crate::settings::migrate_old_settings(&data_dir, &config_dir);
                }
            }
            Ok(())
        })
        .manage(agent_command::SessionManager::new())
        .manage(runtime_env::RuntimeState::default())
        .manage(watcher::WatcherState::default())
        .manage(watcher::SkillWatcherState::default())
        .invoke_handler(tauri::generate_handler![
            commands::read_markdown_file,
            commands::write_markdown_file,
            commands::resolve_path,
            commands::allow_assets,
            commands::read_dir_tree,
            commands::path_exists,
            commands::create_markdown_file,
            commands::create_folder,
            commands::get_default_data_dir,
            commands::join_path,
            commands::open_in_shell,
            commands::check_api_key,
            commands::save_api_key,
            commands::call_ai_provider,
            agent_command::start_agent_turn,
            agent_command::approve_tool_call,
            agent_command::list_skills,
            agent_command::set_skill_active,
            agent_command::load_session_messages,
            agent_command::resolve_session_key,
            agent_command::clear_session,
            agent_command::close_session,
            agent_command::list_sessions,
            watcher::start_watching,
            watcher::stop_watching,
            watcher::start_skills_watching,
            watcher::stop_skills_watching,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
