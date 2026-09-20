mod commands;
mod types;
mod utils;

use commands::create_skill::create_local_skill;
use commands::github_sync::{
    github_sync_status, save_github_sync, sync_github_now, test_github_sync,
};
use commands::history::{list_skill_history, restore_skill_history};
use commands::library_metadata::{get_skill_library_metadata, save_skill_library_entry};
use commands::market::{download_marketplace_skill, search_marketplaces, update_marketplace_skill};
use commands::packages::{
    assign_skill_package_members, delete_skill_package, list_skill_packages,
    reorder_skill_packages, save_skill_package,
};
use commands::skills::{
    adopt_ide_skill, delete_local_skills, detect_ide_locations, discover_skills_in_directory,
    export_local_skills, get_manager_storage_info, import_discovered_skills, import_local_skill,
    link_local_skill, read_local_skill_preview, save_local_skill, scan_overview, uninstall_skill,
};
use commands::skillsmp::search_skillsmp;
use commands::translation_settings::{
    clear_translation_session_key, get_translation_settings, save_translation_settings,
};
use commands::trash::{
    list_trashed_skills, permanently_delete_trashed_skill, restore_trashed_skill,
};
use tauri::Manager;

pub use crate::types::{
    AdoptIdeSkillRequest, BatchImportRequest, BatchImportResult, DeleteLocalSkillRequest,
    DetectIdeLocationsRequest, DiscoveredSkill, ExportSkillsRequest, IdeBrowseLocation, IdeDir,
    IdeSkill, ImportRequest, InstallResult, LinkRequest, LinkTarget, LocalScanRequest, LocalSkill,
    LocalSkillPreview, ManagerStorageInfo, Overview, RemoteSkillView, RemoteSkillsViewResponse,
    SkillDiscoveryRequest, SkillImportItemResult, UninstallRequest,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .setup(|_| {
            commands::github_sync::start_worker();
            Ok(())
        })
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            github_sync_status,
            test_github_sync,
            save_github_sync,
            sync_github_now,
            create_local_skill,
            list_skill_history,
            restore_skill_history,
            get_skill_library_metadata,
            save_skill_library_entry,
            list_trashed_skills,
            restore_trashed_skill,
            permanently_delete_trashed_skill,
            list_skill_packages,
            save_skill_package,
            delete_skill_package,
            assign_skill_package_members,
            reorder_skill_packages,
            search_marketplaces,
            search_skillsmp,
            download_marketplace_skill,
            update_marketplace_skill,
            link_local_skill,
            read_local_skill_preview,
            save_local_skill,
            detect_ide_locations,
            scan_overview,
            uninstall_skill,
            import_local_skill,
            import_discovered_skills,
            discover_skills_in_directory,
            get_manager_storage_info,
            delete_local_skills,
            export_local_skills,
            adopt_ide_skill,
            get_translation_settings,
            save_translation_settings,
            clear_translation_session_key
        ]);

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        // When a second instance is started, focus the existing window
        let _ = app
            .get_webview_window("main")
            .expect("no main window")
            .set_focus();
    }));

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
