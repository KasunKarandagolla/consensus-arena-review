// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agentic_manager;
mod agent_brain;
mod blueprint_store;
mod browser_backend;
mod capability_registry;
mod commands;
mod context_manager;
mod db_helpers;
mod errors;
mod memory_store;
mod orchestrator;
mod persona_manager;
mod proxy_manager;
mod resource_monitor;
mod response_router;
mod session_runner;
mod session_vault;
mod settings_store;
mod signals;
mod token_budget;
mod transcript_store;
mod turn_manager;

use orchestrator::AppState;
use tauri::{Emitter, Manager};

fn main() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("could not resolve app data directory");
            std::fs::create_dir_all(&data_dir)
                .expect("could not create app data directory");

            // ── IMP-8: File-backed tracing (rolling daily log) ────────────────
            let file_appender =
                tracing_appender::rolling::daily(&data_dir, "consensus-arena.log");
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
                )
                .with_writer(non_blocking)
                .with_ansi(false) // no ANSI colour codes in log files
                .init();

            // Keep the background writer thread alive for the process lifetime.
            std::mem::forget(guard);

            // ── AppState ──────────────────────────────────────────────────────
            let data_dir_str = data_dir.to_string_lossy().into_owned();
            let app_state = AppState::new(&data_dir_str);
            if !app_state.last_memory_health.is_healthy
                || app_state.last_memory_health.fts_needs_repair
            {
                let text = if app_state.last_memory_health.issues.is_empty() {
                    app_state.last_memory_health.warnings.join("; ")
                } else {
                    app_state.last_memory_health.issues.join("; ")
                };
                eprintln!("[MEMORY] Health check warning: {text}");
                let _ = app.emit(
                    "memory-health-warning",
                    serde_json::json!({
                        "text": "Memory database issue detected. Some session history may be unavailable.",
                        "fts_needs_repair": app_state.last_memory_health.fts_needs_repair
                    }),
                );
                let _ = app.emit(
                    "boss-message",
                    serde_json::json!({
                        "text": "Memory database issue detected. Some session history may be unavailable.",
                        "message_type": "status"
                    }),
                );
            }
            app.manage(app_state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Session management
            commands::start_session,
            // BUGFIX (Cline audit, post-Batch-D): pause_session and
            // resume_session were fully implemented in commands.rs but were
            // never registered here — the frontend could never actually call
            // them. Found by an independent read-only audit that compared
            // every #[tauri::command] function defined in commands.rs against
            // every command registered in this generate_handler! list; 26
            // functions existed, only 24 were reachable. These two were the
            // gap.
            commands::pause_session,
            commands::resume_session,
            commands::abort_session,
            // User interaction
            commands::user_input,
            commands::captcha_resolved,
            commands::retry_setup_agent,
            commands::confirm_setup_agent,
            commands::provide_manual_model_response,
            commands::rate_limit_decision,
            commands::setup_agent_sent,
            commands::provide_user_answer,          // D-041
            // Settings & configuration
            commands::save_agent_brain_config,
            commands::get_agent_brain_config,
            commands::save_secondary_brain_config,  // D-039
            commands::get_secondary_brain_config,   // D-039
            commands::save_fallback_brain_config,   // Task 5 (HIGH-3)
            commands::get_fallback_brain_config,    // Task 5 (HIGH-3)
            commands::save_prompt_template,
            commands::get_prompt_template,
            commands::get_diagnostic_snapshot,
            // Data retrieval
            commands::get_transcript,
            commands::get_session_list,
            commands::export_blueprint,
            commands::get_agent_health,
            // Task 3 (CRIT-3, CRIT-4): session CRUD
            commands::delete_session,
            commands::rename_session,
            commands::get_session_details,
            // IMP-7: Session recovery
            commands::get_recovery_state,
            commands::recover_session,
            // Phase 1 memory
            commands::get_project_memory,
            commands::get_global_memory,
            commands::clear_project_memory,
            commands::get_open_questions,
            commands::get_model_strengths,
            commands::save_project_config,
            commands::get_project_config,
            commands::get_memory_health,
            commands::repair_memory_index,
            commands::get_patterns,
            commands::export_memory,
            commands::restore_memory,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested { .. } = event {
            let state = app_handle.state::<AppState>();
            if let Ok(mut memory) = state.memory_store.try_lock() {
                if let Err(e) = memory.commit_pending_state() {
                    eprintln!("[MEMORY] Shutdown checkpoint failed: {e}");
                }
            }
        }
    });
}
