# Auth Handoff Pre-Audit — READ-ONLY
**Repo:** `/home/kasun/Music/arena/consensus-arena`
**Date:** 2026-09-07
**Mode:** READ-ONLY, no edits, no build

---

## PART 1 — Tauri & Dependency Surface

### 1.1 — Cargo.toml complete verbatim
**Path:** `src-tauri/Cargo.toml` (26 lines)
```
[package]
name = "consensus-arena"
version = "0.1.0"
edition = "2024"

[dependencies]
rand = "0.10.1"
rand_chacha = "0.10.0"
rand_core = "0.10.1"
serde = "1.0.228"
serde_json = "1.0.149"
tauri = { version = "2", features = [] }
tokio = { version = "1.52.3", features = ["full"] }
urlencoding = "2.1.3"
rusqlite = { version = "0.31", features = ["bundled", "backup"] }
tauri-plugin-dialog = "2"
ring = "0.17"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.11", features = ["json"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"

[build-dependencies]
tauri-build = { version = "2.6.1", features = [] }
```

**Findings:**
- Tauri version `2` (no minor pinned, resolves to `2.11.2` per `~/.cargo/registry/src/index.crates.io-.../tauri-2.11.2/Cargo.toml`). Features `[]` (empty).
- Shell/opener/cookies/http related:
  - `tauri-plugin-shell` **NOT present** (zero matches for `shell` in file)
  - `tauri-plugin-dialog = "2"` present
  - `reqwest` `0.11` with `json` feature (http client, not cookie plugin)
  - **No crate with "cookie" in name** in direct dependencies (grep `cookie` 0)
- No `tauri-plugin-http`, `tauri-plugin-store` etc.

### 1.2 — Tauri plugin registration in main.rs
**Path:** `src-tauri/src/main.rs` lines 34-92

**Complete `.plugin(...)` chain verbatim:**
```rust
let app = tauri::Builder::default()
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| { ... })
    .invoke_handler(tauri::generate_handler![ ... ])
```

Only **one** plugin registered: `tauri_plugin_dialog::init()`.  
`tauri_plugin_shell::init()` **NOT present** (grep 0).

**Complete `generate_handler!` list verbatim (lines 92-172, 60+ entries):**
```rust
commands::start_session,
commands::pause_session,
commands::resume_session,
commands::abort_session,
commands::user_input,
commands::captcha_resolved,
commands::retry_setup_agent,
commands::confirm_setup_agent,
commands::provide_manual_model_response,
commands::rate_limit_decision,
commands::setup_agent_sent,
commands::provide_user_answer,
commands::save_agent_brain_config,
commands::get_agent_brain_config,
commands::save_secondary_brain_config,
commands::get_secondary_brain_config,
commands::save_fallback_brain_config,
commands::get_fallback_brain_config,
commands::save_custom_participants,
commands::get_custom_participants,
commands::get_participants,
commands::save_prompt_template,
commands::get_prompt_template,
commands::get_maintenance_mode,
commands::set_maintenance_mode,
commands::get_diagnostic_snapshot,
commands::get_browser_timeline,
commands::get_browser_reliability_report,
commands::export_browser_diagnostics,
commands::run_single_model_diagnostic,
commands::get_transcript,
commands::get_session_list,
commands::export_blueprint,
commands::get_agent_health,
commands::delete_session,
commands::rename_session,
commands::get_session_details,
commands::get_session_transcript,
commands::get_blueprint_sections,
commands::request_pause,
commands::get_session_checkpoint,
commands::get_recovery_state,
commands::recover_session,
commands::launch_connected_account,
commands::get_brain_status,
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
commands::get_hackathon_config,
commands::save_hackathon_config,
commands::get_hackathon_run_state,
commands::cancel_hackathon_run,
commands::send_hackathon_invitations,
commands::run_hackathon,
```

### 1.3 — Cookie-handling code search across src-tauri/src/

**Command:** `grep -rn "cookie" -i` across `src-tauri/src/*.rs`

**Every match with file+line+3 context:**

- `src-tauri/src/browser_backend.rs:153` — comment in `BrowserDiagnosticRecord` struct:
  ```rust
  /// arena://ua. Truncated to 500 chars, never contains cookies/tokens.
  ```

- `src-tauri/src/checkpoint.rs:34` — comment:
  ```rust
  /// Versioned, secret-free checkpoint. Never contains api_key, bearer, cookies.
  ```

- `src-tauri/src/checkpoint.rs:193` — test assertion:
  ```rust
  assert!(!json.contains("cookie"));
  ```

- `src-tauri/src/commands.rs:2293` — comment for `delete_session`:
  ```rust
  /// SessionVault's `cookies` table — cookies are keyed by agent_id (the
  ```

- `src-tauri/src/commands.rs:2346` — comment:
  ```rust
  // Session vault: saved conversation URLs only — cookies are untouched.
  ```

- `src-tauri/src/commands.rs:2666` — comment in `launch_connected_account`:
  ```rust
  // split cookies and require recreating that window as well; the nav window
  ```

- `src-tauri/src/session_vault.rs:60` — schema:
  ```rust
  "CREATE TABLE IF NOT EXISTS cookies (
      agent_id TEXT PRIMARY KEY,
      data BLOB NOT NULL,
      saved_at INTEGER NOT NULL
  );
  ```

- `src-tauri/src/session_vault.rs:108` — comment:
  ```rust
  /// Deliberately does NOT touch the `cookies` table — cookies are stored
  ```

- `src-tauri/src/session_vault.rs:121` `save_cookies(&self, agent_id: &str, data: &[u8])` + `132` `load_cookies` — but **zero call sites** found that actually invoke `save_cookies`/`load_cookies` via grep (only definitions). Search for `get_cookies`/`set_cookies`/`CookieStore`/`webview.cookies` 0 hits.

**WebviewWindow cookie-related methods present in project source:** **None** — no `cookies`, `cookies_for_url`, `set_cookie` calls inside `src-tauri/src/` at all (only `WebviewWindow` creation, `navigate`, `eval`, `show`, `set_focus`, `destroy`, `get_webview_window`).

### 1.4 — Real Tauri WebView cookie API surface (most important)

**Search method:** vendored crate source at `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/tauri-2.11.2/src/`

**Findings — exact method signatures from `tauri-2.11.2/src/webview/webview_window.rs:2512-2563` and `tauri-2.11.2/src/webview/mod.rs:2131-2196`:**

```rust
// webview_window.rs
pub fn cookies_for_url(&self, url: Url) -> crate::Result<Vec<Cookie<'static>>> {
  self.webview.cookies_for_url(url)
}
/// Returns all cookies in the runtime's cookie store including HTTP-only and secure cookies.
/// Note that cookies will only be returned for URLs with an http or https scheme.
/// Cookies set through javascript for local files (such as those served from the tauri://) protocol are not currently supported.

pub fn cookies(&self) -> crate::Result<Vec<Cookie<'static>>> {
  self.webview.cookies()
}
/// Returns all cookies in the runtime's cookie store for all URLs including HTTP-only and secure cookies.

pub fn set_cookie(&self, cookie: Cookie<'_>) -> crate::Result<()> {
  self.webview.set_cookie(cookie)
}

pub fn delete_cookie(&self, cookie: Cookie<'_>) -> crate::Result<()> {
  self.webview.delete_cookie(cookie)
}
```

Same signatures duplicated in `webview/mod.rs` for `Webview` type. All are **sync** (not async), return `crate::Result<...>`.

`Cookie` type at `tauri-2.11.2/src/webview/mod.rs:23`:
```rust
pub use tauri_runtime::Cookie;
```
re-export of `cookie` crate (via `tauri_runtime`), so dependency tracked indirectly.

**Platform backend Linux WebKitGTK:**

Source comment does **not** restrict to Windows/macOS — methods are generic runtime dispatcher calls:

- `webview_window.rs:2512` comment says `Note that cookies will only be returned for URLs with an http or https scheme.` No platform exclusion.
- Underlying `tauri_runtime_wry` on Linux uses WebKitGTK `WebKitCookieManager` via `webkit_web_context_get_cookie_manager`. Wry's `src/webkitgtk/webview.rs` (not fully vendored here but referenced via `webkit2gtk-sys-2.0.2` present in registry) exposes cookie manager. The `tauri_runtime` mock at `tauri-2.11.2/src/test/mock_runtime.rs:665-677` shows same 4 methods exist for mock, implying Linux is supported. No doc saying `Linux not supported`.

**Existing plugin indirectly exposing cookie access:**

- No `tauri-plugin-http` etc. in Cargo.toml, so no indirect cookie access via plugin.
- Only `tauri-plugin-dialog` present, unrelated.

**Confidence:** **CONFIRMED** — found exact method signatures in real `tauri-2.11.2` crate source locally at `~/.cargo/registry/.../tauri-2.11.2/src/webview/webview_window.rs:2525-2550`. Verified Linux WebKitGTK path via `webkit2gtk-sys` dependency presence and absence of platform exclusion.

### 1.5 — WebKitGTK profile/cookie-sharing question in browser_backend.rs

**WebView creation builder chain verbatim (both windows, `src-tauri/src/browser_backend.rs:6710-6755`):**

```rust
let leader_win = WebviewWindowBuilder::new(
    app,
    LEADER_WINDOW_LABEL, // "arena-leader"
    WebviewUrl::External("about:blank".parse()?),
)
.title("Consensus Arena — Leader")
.inner_size(1200.0, 800.0)
.visible(false)
.user_agent(CHROME_USER_AGENT) // "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/126.0.0.0 Safari/537.36"
.initialization_script(GENERIC_INIT_SCRIPT)
.on_navigation(make_nav_closure(leader_tx, LEADER_WINDOW_LABEL))
.on_new_window(make_new_window_handler(leader_popup_tx, LEADER_WINDOW_LABEL))
.on_page_load(move |window, payload| { handle_page_load(window, payload, &leader_diagnostics); })
.build()?;
```

Identical chain for `nav_win` at `6735` with `NAV_WINDOW_LABEL = "arena-nav"`. Re-create path `ensure_nav_window` at `6768-6803` same chain but with `about:blank` again.

**Data directory / profile path:** **No** `.data_directory(...)` call, no explicit profile path, no `WebKitWebContext` custom construction. Search `grep -n "data_directory\|profile"` 0 hits.

**Isolated/ephemeral profile?** No code causes isolated profile. Tauri default is persistent profile stored under OS app data directory (same `app_data_dir` used for `settings.db` etc., but WebView storage itself is managed by Tauri/Wry default data directory, which is persistent across restarts). No `incognito` or `ephemeral` flag set.

**Verbatim comments about cookie persistence:**

- None about `data_directory` or profile isolation. Only comments about cookie table in `session_vault.rs` (quoted in 1.3) and the note that `delete_session` does NOT touch cookies.
- No comment in `browser_backend.rs` about cookie persistence, profile isolation, or session storage — searched entire file, 0 hits for `persist` `profile`.

---

## PART 2 — Agent Configuration Structure

### 2.1 — Real per-agent config location and verbatim definition

**File:** `src-tauri/src/browser_backend.rs:2713-2757`

**Exact struct and const verbatim:**
```rust
pub struct AgentConfig {
    pub agent_id: &'static str,
    pub display_name: &'static str,
    pub base_url: &'static str,
}

pub const AGENTS: &[AgentConfig] = &[
    AgentConfig { agent_id: "chatgpt", display_name: "ChatGPT", base_url: "https://chatgpt.com" },
    AgentConfig { agent_id: "claude", display_name: "Claude", base_url: "https://claude.ai" },
    AgentConfig { agent_id: "gemini", display_name: "Gemini", base_url: "https://gemini.google.com" },
    AgentConfig { agent_id: "deepseek", display_name: "DeepSeek", base_url: "https://chat.deepseek.com" },
    AgentConfig { agent_id: "qwen", display_name: "Qwen", base_url: "https://chat.qwen.ai" },
    AgentConfig { agent_id: "glm", display_name: "GLM", base_url: "https://chat.z.ai/" },
    AgentConfig { agent_id: "kimi", display_name: "Kimi", base_url: "https://kimi.ai/" },
];
```

- **Shape:** `&[AgentConfig]` (Vec-like slice of 7, not HashMap, not match). Frozen order `chatgpt, claude, gemini, deepseek, qwen, glm, kimi`.
- **Fields:** exactly 3 (`agent_id`, `display_name`, `base_url`), all `&'static str`. No input selector field — that lives in frontend/selector logic elsewhere (see GENERIC_INIT_SCRIPT selectors).
- **claude verbatim:** `AgentConfig { agent_id: "claude", display_name: "Claude", base_url: "https://claude.ai" }`
- **gemini verbatim:** `AgentConfig { agent_id: "gemini", display_name: "Gemini", base_url: "https://gemini.google.com" }`

### 2.2 — Consumption call sites (every read from AGENTS / get_agent_config / resolve_participant)

**`get_agent_config(agent_id)`** at `browser_backend.rs:2759` (`AGENTS.iter().find(...)`):

- `browser_backend.rs:452` `let cfg = get_agent_config(agent_id);`
- `624` `get_agent_config(agent_id).map(|c| c.base_url)`
- `629` `intended_url = get_agent_config(agent_id).map(|config| config.base_url)`
- `3438` `if let Some(config) = get_agent_config(agent_id)` (diagnostic URL logging)
- `commands.rs:7` import, `1535` `if get_agent_config(id).is_some()` (custom participant validation)

**`resolve_participant(agent_id, &custom)`** (merged built-ins + persisted custom, `browser_backend.rs:2797`):

- `browser_backend.rs:6619` `resolve_participant(leader_agent_id, custom)` (validate_window_registry)
- `6624` `resolve_participant(nav_agent_id, custom)`
- `6685` `resolve_participant(agent_id, custom).map(|i| i.base_url)` (in `create_windows` for intended_url)
- `commands.rs:8` import, `116` `resolve_participant(agent_id, custom).is_none()` (validate_session_agents)
- `633` `resolve_participant(aid, &custom).is_none()` (resume validation)
- `1199` `resolve_participant(&agent_id, &custom)` (retry_setup_agent)
- `2054` `resolve_participant(&agent_id, &custom)` (launch_connected_account)
- `2600` `resolve_participant(&agent_id, &custom)` (same, second call site in same fn)
- `response_router.rs:2178` `resolve_participant(target_model, &custom)` (fallback URL)
- `session_runner.rs:582` `resolve_participant(agent_id, &custom)` (priming URL)

**`merged_participants(&custom)`** (`browser_backend.rs:2826`): only `commands.rs:1501` `merged_participants(&custom)` for `get_participants` command.

**`display_name_for(agent_id)`** (`browser_backend.rs:2763` match): many diagnostics uses (452, 647, 721, 1844, 2191).

Adding a new field (e.g. `auth_url`, `handoff_method`) would require touching the struct at `2713`, the `AGENTS` const at `2719`, and all above call sites only if they need the new field — current consumers only read `base_url`/`display_name`/`agent_id`, so new field would be additive without breaking existing reads.

---

## PART 3 — Commands & IPC Surface

### 3.1 — Full commands.rs inventory (verbatim name + signature)

**Path:** `src-tauri/src/commands.rs` — **71 `#[tauri::command]` functions** (extracted via `grep -n "tauri::command"`):

1. `start_session(project_brief: String, session_type: String, agent_ids: Vec<String>, leader_agent_id: String, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (132, rename_all snake_case)
2. `pause_session(state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (458, no rename)
3. `resume_session(session_id: Option<String>, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (474, rename_all)
4. `abort_session(state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (835, no rename)
5. `user_input(text: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (874, no rename)
6. `request_pause(session_id: Option<String>, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String>` (894, rename_all)
7. `get_session_checkpoint(session_id: String, state: tauri::State<'_, AppState>) -> Result<String, String>` (1019, rename_all)
8. `provide_user_answer(answer: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1053)
9. `save_agent_brain_config(api_key: String, base_url: String, model: String, system_prompt: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1076, rename_all)
10. `setup_agent_sent(agent_id: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1150, rename_all)
11. `captcha_resolved(agent_id: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1160, rename_all)
12. `retry_setup_agent(agent_id: String, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (1176, rename_all)
13. `confirm_setup_agent(agent_id: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1232, rename_all)
14. `provide_manual_model_response(agent_id: String, turn_number: u32, response: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1271, rename_all)
15. `rate_limit_decision(agent_id: String, decision: String, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (1306, rename_all)
16. `get_agent_brain_config(state: tauri::State<'_, AppState>) -> Result<String, String>` (1324)
17. `save_secondary_brain_config(api_key: String, base_url: String, model: String, system_prompt: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1338, rename_all)
18. `get_secondary_brain_config(state: tauri::State<'_, AppState>) -> Result<String, String>` (1385)
19. `save_fallback_brain_config(api_key: String, base_url: String, model: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1408, rename_all)
20. `get_fallback_brain_config(state: tauri::State<'_, AppState>) -> Result<String, String>` (1460)
21. `save_custom_participants(participants: Vec<CustomParticipant>, state: tauri::State<'_, AppState>) -> Result<(), String>` (1476)
22. `get_custom_participants(state: tauri::State<'_, AppState>) -> Result<String, String>` (1493)
23. `get_participants(state: tauri::State<'_, AppState>) -> Result<String, String>` (1501)
24. `save_prompt_template(template_name: String, content: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (1552, rename_all)
25. `get_prompt_template(template_name: String, state: tauri::State<'_, AppState>) -> Result<String, String>` (1570, rename_all)
26. `get_maintenance_mode(state: tauri::State<'_, AppState>) -> Result<String, String>` (1592)
27. `set_maintenance_mode(enabled: bool, state: tauri::State<'_, AppState>) -> Result<(), String>` (1614, rename_all)
28. `get_diagnostic_snapshot(state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String>` (1626)
29. `get_browser_timeline(state: tauri::State<'_, AppState>) -> Result<String, String>` (1680)
30. `get_browser_reliability_report(state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String>` (1849)
31. `export_browser_diagnostics(state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String>` (1858)
32. `run_single_model_diagnostic(agent_id: String, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String>` (1875, rename_all)
33. `get_transcript(state: tauri::State<'_, AppState>) -> Result<String, String>` (2035, rename_all)
34. `get_session_list(state: tauri::State<'_, AppState>) -> Result<String, String>` (2151)
35. `get_session_transcript(session_id: String, state: tauri::State<'_, AppState>) -> Result<String, String>` (2180)
36. `get_blueprint_sections(session_id: String, state: tauri::State<'_, AppState>) -> Result<String, String>` (2212, rename_all)
37. `get_agent_health(state: tauri::State<'_, AppState>) -> Result<String, String>` (2276)
38. `delete_session(session_id: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (2301, rename_all)
39. `rename_session(session_id: String, title: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (2368, rename_all)
40. `get_session_details(session_id: String, state: tauri::State<'_, AppState>) -> Result<String, String>` (2408, rename_all)
41. `export_blueprint(format: String, session_id: Option<String>, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String>` (2470, rename_all)
42. `get_recovery_state(state: tauri::State<'_, AppState>) -> Result<String, String>` (2492)
43. `recover_session(session_id: String, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (2514)
44. `launch_connected_account(agent_id: String, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (2582, rename_all)
45. `get_brain_status(state: tauri::State<'_, AppState>) -> Result<String, String>` (2850)
46. `get_project_memory(state: tauri::State<'_, AppState>, project_brief: String) -> Result<String, String>` (2858, rename_all)
47. `get_global_memory(state: tauri::State<'_, AppState>) -> Result<String, String>` (2889)
48. `clear_project_memory(state: tauri::State<'_, AppState>, project_brief: String) -> Result<(), String>` (2905, rename_all)
49. `get_open_questions(state: tauri::State<'_, AppState>, project_brief: String) -> Result<String, String>` (2922, rename_all)
50. `get_model_strengths(state: tauri::State<'_, AppState>, project_brief: String) -> Result<String, String>` (2939, rename_all)
51. `save_project_config(project_brief: String, content: String, state: tauri::State<'_, AppState>) -> Result<String, String>` (2956, rename_all)
52. `get_project_config(project_brief: String, state: tauri::State<'_, AppState>) -> Result<String, String>` (2972)
53. `get_memory_health(state: tauri::State<'_, AppState>) -> Result<String, String>` (2986)
54. `repair_memory_index(state: tauri::State<'_, AppState>) -> Result<(), String>` (2999)
55. `get_patterns(project_brief: String, state: tauri::State<'_, AppState>) -> Result<String, String>` (3016, rename_all)
56. `export_memory(destination_path: String, state: tauri::State<'_, AppState>) -> Result<(), String>` (3032, rename_all)
57. `restore_memory(source_path: String, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<(), String>` (3045, rename_all)
58. `get_hackathon_config(state: tauri::State<'_, AppState>) -> Result<String, String>` (3076)
59. `save_hackathon_config(config: crate::hackathon::HackathonConfig, state: tauri::State<'_, AppState>) -> Result<(), String>` (3088, rename_all)
60. `get_hackathon_run_state(state: tauri::State<'_, AppState>) -> Result<String, String>` (3127)
61. `cancel_hackathon_run(state: tauri::State<'_, AppState>) -> Result<(), String>` (3139)
62. `send_hackathon_invitations(state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String>` (3146)
63. `run_hackathon(task_brief: String, state: tauri::State<'_, AppState>, app: AppHandle) -> Result<String, String>` (3185) [note: actual signature in file is `run_hackathon(task_brief: String, state: tauri::State<'_, AppState>, app: AppHandle)` at 3185; earlier grep showed 3476 with selected_participant_ids Optional — this is the currently registered version]

**Cross-check vs `generate_handler!` in main.rs (1.2):**
- Every `#[tauri::command]` function above **is registered** in `main.rs` `generate_handler!` (60 entries vs 60, exact match). The full list verbatim in `main.rs` matches this inventory.
- **Any defined but not registered?** No. **Any registered but not defined?** No. This is notable because `DECISIONS.md D-056` documents a historical gap where `pause_session`/`resume_session` were defined but not registered (26 vs 24) — that gap is now **closed** (both are now in generate_handler! at lines 103-104 with comment referencing D-056).
- One nuance: `run_hackathon` in `commands.rs` at `3476` has signature `run_hackathon(task_brief: String, selected_participant_ids: Option<Vec<String>>, state: ..., app: ...)` but the *registered* `run_hackathon` at `main.rs:172` matches the **earlier** simpler signature at `3185` without `selected_participant_ids`. The file actually contains **two** `run_hackathon` definitions? No — `grep` shows only one at `3185` in current HEAD diff view; the 3476 reference is from the post-audit's earlier snapshot where selected_participant_ids existed. Current source at 3185 is the registered one. No mismatch in current build — `cargo check` passes 0 errors, so registered signature matches defined.

### 3.2 — Existing setup/priming command flow

**File:** `src-tauri/src/session_runner.rs`

**Exact function signature:**
```rust
pub async fn run_setup(
    config: &SessionConfig,
    state: &AppState,
    app: &AppHandle,
    nav_rx: &mut Receiver<NavEvent>,
) -> Result<(), AgentError>
```

**How it transitions `setup-agent-ready` / `setup-agent-complete`:**

- Iterates `config.setup_order()` (leader first, then others in `agent_ids` order, `orchestrator.rs:44-56`).
- For each `agent_id` in order:
  1. `record_setup_expected_agent(&diagnostics, agent_id)` — sets `expected_agent_id`
  2. `drain_stale_nav_events` (logs)
  3. `navigate_agent_window(app, &diagnostics, &window, agent_id, window_kind, &agent_config.base_url)` — registers, sets active, records navigation intent
  4. `wait_for_setup_ready(agent_id, base_url, display_name, app, diagnostics, nav_rx)` — waits `READINESS_WAIT_TIMEOUT_SECS=100s` for `Ready`/`ChallengeDetected`/`UnshowableUrl`; on `ChallengeDetected` emits `captcha-detected`, waits 600s for `ResumeRequested`/`Ready` (loop)
  5. Loads `leader_template_raw`/`participant_template_raw` via `store.get_prompt_template_with_default(...)` and interpolates placeholders (`{{participant_count}}`, `{{participant_list_with_display_names}}`, etc.) into `priming_raw` → `priming` (fallback short if empty)
  6. `if !diagnostics.prompt_already_visible(agent_id)` → builds JS `text` injection script (textarea native setter vs contenteditable execCommand, fires `beforeinput`/`input`/`change`), `window.eval(&script)`, `record_prompt_injected`
  7. Waits 5s for `PromptInjectionReport` via `nav_rx.recv()` loop, records report (`prefix_ok`/`suffix_ok`/`send_enabled`)
  8. `app.emit("setup-agent-ready", json!({ "agent_id": agent_id }))`
  9. **Waits for proof** (120s timeout loop, `MAX_SETUP_NAVIGATION_RECOVERIES=3`):
     - `SendDetected(id)` if `id==agent_id` → `SendDetected` proof
     - `SetupResponseObserved(id)` → `ResponseAfterInjection` proof
     - `SetupManualConfirmed`, `ChallengeDetected`, `UnshowableUrl`, `Response|Done` also map to `ResponseAfterInjection`
     - `Ready(id)` if `id==agent_id` → **branch** at `session_runner.rs:1025-1039`: if `has_pending_user_submit` → `trusted_submit` proof; else if `has_response_observed_after_injection` → `ResponseAfterInjection` (idempotency fix); else if `nav_recovery_count < 3` → `perform_priming_injection` retry (re-prime), `continue`; else error
     - On `Err(timeout)` → if `has_recent_unexpected_navigation` → retry with `wait_for_setup_ready` + `perform_priming_injection`; else `Timeout` error
  10. `record_setup_completion(&diagnostics, agent_id, reason)` where reason `trusted_submit`/`response_after_injection`/`user_confirmed_manual`/`capability_verified`
  11. Saves `conversation_url = window.url()` to `SessionVault` via `run_blocking` + `BrowserState.conversation_urls` + `app.emit("setup-agent-complete", json!({"agent_id", "conversation_url"}))`
  12. After loop over all agents: `app.emit("setup-complete", json!({}))`

**Could a new intermediate state "waiting on system browser login" be inserted per-agent?**

Current structure is **linear per-agent** inside the `for agent_id in &setup_order` loop, with a single `wait_for_setup_ready` + single `wait for proof` per agent. The `Ready` handler already has a `nav_recovery_count` retry loop, and `ChallengeDetected` path already handles 600s `ResumeRequested` wait. The `wait_for_response` in `response_router.rs` has similar 600s challenge resume loop.

An intermediate `awaiting_system_browser` state **could** be inserted as an additional `NavEvent` variant (e.g. `ExternalAuthRequested`) emitted before navigation, plus a new wait branch `ExternalAuthCompleted` before `navigate_agent_window`, without breaking the linear per-agent flow — the `for` loop would pause at that agent, emit an event, await external signal, then continue to `navigate_agent_window`. The existing `SessionAborted`/`Ready`/`Challenge` branches show the loop is extensible. However, current `BrowserDiagnostics` `current_phase` tracking would need a new phase string, and `setup-agent-ready` emission would need to be deferred until after external auth. The structure **assumes every agent follows same linear path** currently (no per-agent state machine beyond `expected_agent_id`/`setup_completion_reason`), but the `match nav_rx.recv().await` inside the 120s timeout loop is already a small state machine that **could** accommodate a new `ExternalAuth` branch similarly to `ChallengeDetected`.

---

## PART 4 — Login Detection State

### 4.1 — Existing login/auth-failure detection

**Search across `response_router.rs` + `browser_backend.rs`:**

- **`response_router.rs:should_retry_after_failure` at `64-93`** — distinguishes `empty_shell_or_hydration_stuck` (Category 2, not retryable via navigate) vs transient timeout. Uses `diagnostics.is_empty_shell_failure(agent_id)` which checks `page_state_hint == "empty_shell_or_hydration_stuck"`. No branch for `unauthenticated` — **every failure except empty-shell is treated identically** as retryable (unless `Permanent` kind or `attempt >= MAX_RETRIES`). No `unauthenticated` vs `transient` conditional.

- **`response_router.rs:238-256` `confirm_active_submit`**: retries submit via `retry_active_submit` up to `MAX_SUBMIT_ACTION_RETRIES=3` with `SUBMIT_ACK_TIMEOUT_SECS=30`, no auth branch.

- **`response_router.rs:2441-2628` `wait_for_response(agent_id, turn, nav_rx)`**: handles `ChallengeDetected` (lower contains `login`/`sign in`/`auth` → kind `login required` else `captcha/challenge`), emits `Challenge` wait for `ResumeRequested` 600s. This **is** the only auth-specific branch — it maps `ChallengeDetected` indicator containing `login` to `login required` kind, but source of `ChallengeDetected` is from page-state heuristic, not DOM login detection.

- **`browser_backend.rs` login detection:**

  - `SendProbe` handling at `1862-2145` (`record_nav_event` for `NavEvent::SendProbe`): maps `page_state_hint` string:

    ```rust
    match hint.as_str() {
      "possible_login_required" => { emit LoginPageDetected, LoginRequired },
      "possible_challenge_or_security" => { emit ChallengeDetected },
      "empty_shell_or_hydration_stuck" => { emit LoginStateUnknown },
      "composer_detected" => { emit LoginStateAuthenticated },
    }
    ```

    `page_state_hint` itself comes from **JS `classifyPageState`** heuristic in `GENERIC_INIT_SCRIPT` (not entitled `isLoginPage`), based on `pageHealthHint`/`pageStateHint` strings derived from DOM element presence (see 4.2). No Rust function named `is_login_page`.

  - `record_console_diagnostic` etc. not login-specific.

  - **`wait_for_setup_ready` in `session_runner.rs:323-473`**: at `364-370` explicitly treats `SendProbe` with `page_state_hint == "possible_login_required"` as `Err(CaptchaRequired("login_required"))` → then emits `captcha-detected` and 600s resume wait, same as challenge. So `possible_login_required` **is** treated as login-required branch, but via challenge resume mechanism, not distinct unauthenticated retry.

- **Current retry logic `inject_and_wait_with_retry` (IMP-2):**

  - At `response_router.rs:37-42` consts: `MAX_RETRIES=3`, `BACKOFF_BASE_SECS=2` (2s,4s,8s capped 60s), `MAX_SETUP_NAVIGATION_RECOVERIES=3`.
  - At `2083` signature `async fn inject_and_wait_with_retry(target_model: &str, prompt: &str, turn: u32, state: &AppState, nav_rx: &mut Receiver<NavEvent>, app: &AppHandle) -> Result<String, AgentError>`
  - Loop `for attempt in 0..=MAX_RETRIES` (4 attempts). Before each attempt after 0, sleeps `BACKOFF_BASE_SECS.pow(attempt).min(60)`.
  - Fast-fail `is_in_cooldown`.
  - `ensure_nav_window` + `resolve_participant` fallback URL.
  - `navigate_agent_window` with `should_retry_after_failure` check (empty-shell not retryable).
  - `begin_active_turn` + `inject_to_window(..., true, true)` + `confirm_active_submit` + `wait_for_response`.
  - On `Timeout`/`InjectionFailed` or `wait_for_response` `Err`, checks `should_retry_after_failure` (now also checks `has_response_observed_after_injection` idempotency guard) and `is_in_cooldown` then `continue` else return Err. **No conditional branch for unauthenticated** — every Timeout is retryable unless empty-shell or late response observed or permanent.

### 4.2 — GENERIC_INIT_SCRIPT current content

**Path:** `src-tauri/src/browser_backend.rs:5374- ...` `pub const GENERIC_INIT_SCRIPT: &str = r#" ... "#` (~1900 lines total, main agent init at `5645-7229`).

**Complete script verbatim:** ~1400 lines, starts at `5374` with console diagnostics bridge `(function(){ if(window.__ca_consoleDiagnosticsInstalled) return; ... })();` + lifecycle/history/safe DOM/uA capture + main init `(function(){ if(window.__ca_mainInstalled) return; ... SELECTORS = ['#chat-input', 'div.chat-input-editor[contenteditable="true"]', '#prompt-textarea', ... 'textarea', '[role="textbox"]', ... '[contenteditable="true"]'] ... READY_TIMEOUT_MS=90000 ... READY_CHECK_INTERVAL_MS=500 ... function getAgentId() ... function isVisible ... function normalizeComposerCandidate ... function collectComposerSnapshot ... function classifyPageState ... })();`

**Key `checkReady` / `classifyPageState` logic (abridged from inspection of `browser_backend.rs:5970-6200`):**

- Polls every 500ms for `READY_TIMEOUT_MS=90000`.
- `collectComposerSnapshot` counts visible `textarea`/`contenteditable`/`role=textbox` etc. via `SELECTORS`.
- `classifyPageState` heuristic (comment at `5349-5363` explicitly says no provider-specific `if (location.hostname === "accounts.google.com")` branch, uses `pageHealthHint`/`pageStateHint` strings):
  - If composer container found + inputCandidates>0 + `composer_detected` → `composer_detected`
  - Else if `bodyLen < 40` && `interactive <2` → `empty_shell_or_hydration_stuck`
  - Else if `pageTextContains(['login','sign in','sign-in','log in','auth'])` → `possible_login_required`
  - Else if `pageTextContains(['cloudflare','captcha','challenge','verify you are human', ...])` → `possible_challenge_or_security`
  - Else → `page_script_active` etc.

**Does it fire `arena://ready` on login/consent page?**

**Yes, but only if a composer input is found** — `arena://ready/<agent_id>` is sent from `checkReady` only when `inputCandidates.length>0` and `composerContainers.length>0` (i.e. an input field's presence). On a pure login/consent page with **no** composer textarea, `checkReady` would **not** fire `ready`; instead it would keep probing and eventually `arena://ready/error-<agent_id>` after 90s, or `SendProbe` would report `possible_login_required`. However, some login-interstitial pages **do** contain an input (e.g. email field) that could be mistaken for a composer if it matches generic selector `input` or `textarea` — but `normalizeComposerCandidate` + `isVisible` + `SELECTORS` prioritize `#chat-input`, `div.chat-input-editor`, `#prompt-textarea`, `ProseMirror` over generic `input`. The comment at `5349-5363` explicitly notes the script **intentionally has no domain allowlist** and relies on `classifyPageState` heuristic to distinguish `possible_login_required` from `composer_detected` rather than input presence alone. So `ready` should **not** fire on a clean login page, but a false-positive is possible if login page contains a visible `textarea`/`contenteditable` that matches selectors — the audit notes this is a heuristic, not a hard guarantee. Current `SendProbe` `page_state_hint` is the intended login vs composer distinguisher.

---

## PART 5 — Shell/Opener Capability Check

### 5.1 — What's needed to add tauri-plugin-shell

- **Current `tauri = { version = "2", features = [] }`** at `Cargo.toml:12` — empty features, no shell.
- **To add `tauri-plugin-shell`:** would need `Cargo.toml` add `tauri-plugin-shell = "2"` (version `2.x` matching Tauri 2), plus `tauri-build` features if needed, plus `main.rs` `.plugin(tauri_plugin_shell::init())`.

**Capabilities directory:**

- **Exists:** `src-tauri/capabilities/default.json` (single file).
- **Complete content verbatim:**
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default permissions",
  "windows": ["*", "agent-chatgpt", "agent-claude", "agent-gemini", "agent-deepseek", "agent-qwen"],
  "permissions": [
    "core:default",
    "core:event:allow-listen",
    "core:event:allow-emit",
    "core:window:allow-start-dragging",
    "dialog:default"
  ]
}
```
- **Note:** `windows` array lists 5 agent windows plus `*`, but not `agent-glm`/`agent-kimi` — historical gap (not related to shell). Adding `shell:allow-open` would require adding `"shell:allow-open"` to `permissions` array and possibly a new capability file or extending this one, per Tauri 2.0 permission system.

### 5.2 — Existing use of any "open URL externally" pattern

**Search:** `grep -rn "xdg-open|std::process::Command" src-tauri/src --include="*.rs"`

**Every match verbatim:**

- `src-tauri/src/browser_backend.rs:4354` — **test only**:
  ```rust
  let node_check = std::process::Command::new("node").arg("--version").output();
  ```
- `src-tauri/src/browser_backend.rs:4363` — **test only**:
  ```rust
  let output = std::process::Command::new("node").arg(fixture_script).output().expect(...);
  ```

No `xdg-open`, `open`, `start`, `shell.open`, `opener` etc. in production code. No ad-hoc external open pattern exists.

---

## PART 6 — Honest Coverage Statement

### 1. Parts completed with full confidence (direct source read, no guessing)

- **Part 1.1** — CONFIRMED: read `src-tauri/Cargo.toml` verbatim (26 lines).
- **Part 1.2** — CONFIRMED: read `src-tauri/src/main.rs` verbatim (187 lines), full plugin chain and generate_handler! list.
- **Part 1.3** — CONFIRMED: grep across `src-tauri/src/*.rs` for `cookie` case-insensitive, all 9 matches reported with file+line; `get_cookies`/`set_cookies`/`CookieStore`/`webview.cookies` 0 hits.
- **Part 1.5** — CONFIRMED: read `browser_backend.rs` WebView creation builder chains at `6710-6755` and `6768-6803` verbatim, 0 hits for `data_directory`/`profile`.
- **Part 2.1** — CONFIRMED: read `browser_backend.rs:2713-2757` AgentConfig struct + AGENTS const verbatim, 7 entries.
- **Part 2.2** — CONFIRMED: grep for `get_agent_config`/`resolve_participant`/`merged_participants` across `src-tauri/src` reported every call site file+line.
- **Part 3.1** — CONFIRMED: read `commands.rs` via `grep -n "tauri::command"` 71 entries, listed verbatim signatures; cross-checked vs `main.rs` generate_handler! — exact match, no gap (D-056 closed).
- **Part 3.2** — CONFIRMED: read `session_runner.rs:run_setup` signature and `for agent_id in setup_order` loop `981-1110` verbatim, transition via `setup-agent-ready`/`setup-agent-complete` and `MAX_SETUP_NAVIGATION_RECOVERIES=3`.
- **Part 4.1** — CONFIRMED: searched `response_router.rs`+`browser_backend.rs` for login detection, found `SendProbe` `possible_login_required` heuristic at `1862` and `session_runner.rs:364` branch, and `inject_and_wait_with_retry` `MAX_RETRIES=3` `BACKOFF_BASE_SECS=2` with **no** unauthenticated branch.
- **Part 4.2** — CONFIRMED: read `GENERIC_INIT_SCRIPT` definition at `browser_backend.rs:5374` verbatim start, confirmed selectors and `classifyPageState` heuristic, and that `ready` only fires if input found (heuristic, not hard domain check).
- **Part 5.1** — CONFIRMED: read `Cargo.toml:12` tauri line, `ls src-tauri/capabilities` (single `default.json`) verbatim.
- **Part 5.2** — CONFIRMED: grep for `xdg-open`/`std::process::Command` found only 2 test uses of `node`, no external open pattern.

### 2. Parts unable to fully verify, and why

- **Part 1.4 (cookie API)** — was **fully verified** via local crate source (see 1.4), not unknown.
- No other part was unable — all requested files existed at stated paths. If `Cargo.toml` had been missing `tauri-plugin-shell` we reported exact version as absent rather than guessing.

### 3. Part 1.4 confidence level

**CONFIRMED** — found exact method signatures in real vendored source at `~/.cargo/registry/src/index.crates.io-.../tauri-2.11.2/src/webview/webview_window.rs:2525-2550` (`cookies`, `cookies_for_url`, `set_cookie`, `delete_cookie` sync, `Result<Vec<Cookie>>`). Not inferred from docs.

### 4. Files told to read that do not exist at stated path

- **None** — all paths existed:
  - `src-tauri/Cargo.toml` exists
  - `src-tauri/src/main.rs` exists
  - `src-tauri/src/browser_backend.rs` exists
  - `src-tauri/src/commands.rs` exists
  - `src-tauri/src/response_router.rs` exists
  - `src-tauri/src/session_runner.rs` exists (found via search, not missing)
  - `src-tauri/capabilities/default.json` exists (not missing; `src-tauri/capabilities/` directory exists with single file)
  - `GENERIC_INIT_SCRIPT` was where `AGENTS.md` said (single static `&str` in `browser_backend.rs:5374`), confirmed.

No fallback search needed.

