// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_brain;
mod agentic_manager;
mod blueprint_store;
mod browser_backend;
mod browser_harness;
mod browser_lifecycle;
mod candidate_review;
mod capability_registry;
mod checkpoint;
mod commands;
pub mod consultation;
mod context_manager;
mod credentials;
mod critical_transport;
mod db_helpers;
mod delivery;
mod diagnostics_retention;
mod doc_drift;
mod dsh_worker;
mod errors;
pub mod evidence_gates;
mod execution_profiles;
mod git_runtime;
mod hackathon;
mod memory_store;
mod opencode_adapter;
mod orchestrator;
mod persona_manager;
pub mod pipeline_contract;
mod pipeline_ids;
pub mod product_os;
mod product_os_coordinator;
mod product_os_runtime;
mod proxy_manager;
mod quality_workflows;
mod repo_intelligence;
mod resource_monitor;
mod response_router;
mod session_runner;
mod session_runtime;
mod session_vault;
mod settings_store;
mod signals;
mod token_budget;
mod transcript_store;
mod turn_manager;
mod verification;

use orchestrator::AppState;
use tauri::{Emitter, Manager};

fn main() {
    #[cfg(unix)]
    if std::env::args_os().nth(1).as_deref()
        == Some(std::ffi::OsStr::new("--arena-internal-process-group-guard"))
    {
        loop {
            std::thread::park();
        }
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("could not resolve app data directory");
            std::fs::create_dir_all(&data_dir)
                .expect("could not create app data directory");
            if let Err(error) = crate::diagnostics_retention::prune_diagnostic_logs(
                &data_dir,
                std::time::SystemTime::now(),
            ) {
                eprintln!("[DIAGNOSTICS] Could not prune expired app logs: {error}");
            }

            // ── IMP-8: File-backed tracing (rolling daily log) ────────────────
            let file_appender = tracing_appender::rolling::Builder::new()
                .rotation(tracing_appender::rolling::Rotation::DAILY)
                .filename_prefix("consensus-arena.log")
                .max_log_files(15)
                .build(&data_dir)?;
            let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .with_writer(non_blocking)
                .with_ansi(false) // no ANSI colour codes in log files
                .init();

            // Keep the background writer thread alive for the process lifetime.
            std::mem::forget(guard);

            // ── AppState ──────────────────────────────────────────────────────
            let data_dir_str = data_dir.to_string_lossy().into_owned();
            let app_state = AppState::new(&data_dir_str, app.handle());
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
            commands::start_delivery,
            commands::get_dsh_prerequisite,
            commands::get_delivery_state,
            commands::get_delivery_recovery_state,
            commands::create_product_research_work_order,
            commands::run_product_research_work_order,
            commands::create_product_web_research_work_order,
            commands::run_product_web_research_work_order,
            commands::create_product_fact_verifier_work_order,
            commands::run_product_fact_verifier_work_order,
            commands::create_product_web_fact_verifier_work_order,
            commands::run_product_web_fact_verifier_work_order,
            commands::cancel_product_work_order,
            commands::get_product_os_snapshot,
            commands::get_latest_product_os_snapshot,
            commands::admit_product_ambiguity,
            commands::provide_product_owner_decision,
            commands::start_product_project,
            commands::get_product_coordinator_status,
            commands::answer_product_question,
            commands::cancel_product_project,
            commands::resume_product_project,
            commands::resume_delivery,
            commands::abort_delivery,
            commands::apply_delivery,
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
            commands::clear_brain_credential,
            commands::get_credential_storage_status,
            commands::save_custom_participants,     // P1
            commands::get_custom_participants,      // P1
            commands::get_participants,             // P3 unified registry
            commands::save_prompt_template,
            commands::get_prompt_template,
            commands::get_maintenance_mode,
            commands::set_maintenance_mode,
            commands::get_diagnostic_brief,
            commands::get_diagnostic_snapshot,
            commands::get_browser_timeline,
            commands::get_browser_reliability_report,
            commands::export_browser_diagnostics,
            commands::run_single_model_diagnostic,
            // Data retrieval
            commands::get_transcript,
            commands::get_session_list,
            commands::export_blueprint,
            commands::get_agent_health,
            // Task 3 (CRIT-3, CRIT-4): session CRUD
            commands::delete_session,
            commands::rename_session,
            commands::get_session_details,
            commands::get_session_transcript,
            commands::get_blueprint_sections,
            commands::request_pause,
            commands::get_session_checkpoint,
            // IMP-7: Session recovery
            commands::get_recovery_state,
            commands::recover_session,
            // Connected Accounts Launch + Brain status
            commands::launch_connected_account,
            commands::get_brain_status,
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
            // Hackathon Mode
            commands::get_hackathon_config,
            commands::save_hackathon_config,
            commands::get_hackathon_run_state,
            commands::cancel_hackathon_run,
            commands::send_hackathon_invitations,
            commands::run_hackathon,
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
