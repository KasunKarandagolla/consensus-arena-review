# Differential Runtime Repair — Pre-Audit (2026-09-07)
Repo: /home/kasun/Music/arena/consensus-arena — corrective session after false source-level pass

## Evidence that prior PASS is false

User runtime screenshot shows leader WebView actually received:
```
You are the leader of a small expert panel inside Consensus Arena.

The user will give a project brief. Your job is to produce a concise, practical blueprint.

You may:
- Ask ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, or Kimi for input.
...
Project brief:
{{PROJECT_BRIEF}}
Session type:
{{SESSION_TYPE}}
Available participants:
{{AGENTS}}
```
Canonical `leader_priming.md` (463 lines) contains `You are the leader of an expert AI panel assembled to design a complete, production-ready project blueprint...`, placeholders `{{project_brief}}` lowercase, `{{participant_count}}`, `{{participant_list_with_display_names}}`, plus sections Runtime state authoritative, Route, RouteCompare, Ask User, Hackathon, Phase1/2, Independent judgment, Handling disagreement, Quality bar, Global completion — none present in runtime text.

Therefore provenance chain broken.

## Provenance chain as coded (pre-fix) — full trace

```
1. Canonical file: /home/kasun/Music/arena/consensus-arena/leader_priming.md
   - 463L, header "# leader_priming — Leader..." + "`template_name: 'leader_priming'`" + "---" + body
2. Compile include: settings_store.rs:9  const DEFAULT_LEADER_PRIMING_RAW: &str = include_str!("../../leader_priming.md");
   - same for participant (10) and agent_system (11)
3. Extract: fn extract_prompt_body(raw)  — find "\n---\n" then trim_start. Returns body only (leader body starts "You are the leader of an expert AI panel...")
   - length canonical leader ~ 18k, participant ~ 9k, agent_system ~ 13k
4. SettingsStore::new(db_path) seeding: seeds keys ("brain_system_prompt", "prompt_leader_priming", "prompt_participant_priming") if get(key) is None or empty.trim(). Uses `_ = store.set(key, value)` ignore errors. Version 2 migration follows.
5. SQLite: settings.db table settings(key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER) — keys `prompt_leader_priming`, `prompt_participant_priming`, `brain_system_prompt`, `prompt_hardening_version`.
6. Retrieval:
   - get_prompt_template(template_name) in commands.rs:1571 uses get_prompt_template_with_default (230) — if raw.trim().is_empty() return Ok(raw) else match key to default.
   - get_agent_brain_config() also falls back if empty to default_agent_system etc (203-214).
   - SettingsPanel.tsx: invoke get_prompt_template('leader_priming') plain string (no JSON.parse) displays.
7. Session configuration: SetupView.tsx start() collects selected ids + leader, invokes start_session(project_brief, session_type, agent_ids, leader_agent_id). Backend validate_session_agents against merged registry.
8. Prompt selection: session_runner::run_setup (493-679) loads templates via:
   ```
   let (leader_template_raw, participant_template_raw) = {
     let store = state.settings_store.lock().await;
     let leader = store.get_prompt_template_with_default("prompt_leader_priming").unwrap_or_else(|_| default_leader_priming());
     let participant = store.get_prompt_template_with_default("prompt_participant_priming").unwrap_or_else(|_| default_participant_priming());
     (leader, participant)
   };
   ```
9. Interpolation: run_setup builds strings:
   - leader: replace {{participant_count}} → other_count, {{participant_list_with_display_names}} → other_list, {{leader_display_name}}, {{full_participant_list_including_leader}}, {{project_brief}}, {{session_type}}, {{role}}
   - participant: similar with {{participant_count}} full, etc.
   - Uses format_display_list for Oxford comma.
10. Final string: `priming_raw` then fallback if empty → short generic (669-679) else priming_raw. Then `priming_json = serde_json::to_string(&priming)` etc., then either `perform_priming_injection` or inline window.eval script builds JS that injects via textarea native setter or contenteditable execCommand, then reports `arena://prompt-injection/...` and `arena://setup-response`.
11. Injection JS payload: built in session_runner.rs:56-194 build_priming_script + 694-814 inline script, sets `text` constant, finds input, injects, verifies visibleText contains text.slice(0,32) and suffix.
12. Model-visible prompt: whatever `text` was. If canonical, model sees 400+ line methodology; if legacy, sees short 10-line.

## Every prompt-producing function / fallback / constant — search results

- SettingsStore: default_leader_priming(), default_participant_priming(), default_agent_system(), extract_prompt_body(), get_prompt_template_with_default(), get_agent_brain_config() — all fallback to canonical when empty.
- Session_runner: build_priming_script() (56), perform_priming_injection() (196), inline injection scripts (694, 814) — fallback generic short at 669-679: `"You are participating in a structured expert panel discussion...Your role is {}. Respond thoughtfully..."` — only fires if priming_raw empty (should never with canonical).
- ContextManager: build_prompt_for_agent() (52) — old simple history builder `"You are participating in a structured expert panel discussion.\nYour role is {}. Agent ID: {}...\nProject Brief:\n..."` — DEFINED ONLY, no call sites (grep shows 1 definition). Dead code, not runtime.
- AgentBrain: DECISION_JSON_CONTRACT (12-33) appended to system_prompt via build_effective_system_prompt (260) — adds action JSON examples + hackathon docs, not replacement. decide() sends effective_system_prompt as system message.
- No occurrence of `"You are the leader of a small expert panel"` nor `{{AGENTS}}` nor `{{PROJECT_BRIEF}}` uppercase nor `"Ask ChatGPT, Claude, Gemini, DeepSeek, Qwen, GLM, or Kimi"` in current src-tauri/src/*.rs or src/*.tsx (grep 0 across rs/ts/md). Provenance break is NOT hardcoded in current source — it is persisted old value in settings.db.
- Legacy placeholders searched: `{{AGENTS}}` 0 hits, `{{PROJECT_BRIEF}}` 0 hits, `{{SESSION_TYPE}}` 0 hits, `{{project_brief}}` hits in session_runner (655,665) and migration checks (127,131) only — runtime template placeholders are lowercase canonical. `{{participant_list_with_display_names}}` in leader_priming.md + session_runner (651,663). `{{participant_count}}` in leader md + session_runner (650,661). `{{leader_display_name}}` in participant md + session_runner (652,660). So current code uses correct placeholders.
- Old placeholder syntax (`{{AGENTS}}` uppercase) only appears in user's runtime evidence, not source — indicates DB content from pre-canonical version (pre-2026-09-07).

## Migration logic audit — root cause of persistence

SettingsStore::new seeds only if `get(key)` is None or empty (99-105):
```rust
match store.get(key) {
  Ok(Some(existing)) if !existing.trim().is_empty() => {} // preserve
  Ok(_) => { let _ = store.set(key, value); }
}
```
If legacy short prompt exists (non-empty), seeding skips.

Migration version 2 (116-162):
```
is_old_leader_factory(s) = (s.contains("leader_priming") && !contains("Runtime state is authoritative"))
                        || (contains("Runtime state is authoritative") && !contains("{{project_brief}}"))
is_old_participant_factory(s) = (contains("participant_priming") && !contains("Runtime context is authoritative"))
                             || (contains("Runtime context is authoritative") && !contains("{{project_brief}}"))
is_old_agent_factory(s) = contains("agent_system") && (!contains("hackathon") || !contains("Roster is authoritative"))
```
Legacy short prompt `You are the leader of a small expert panel inside Consensus Arena... {{AGENTS}}` contains **neither** `leader_priming` header nor `Runtime state is authoritative`, so both conditions false → NOT old → NOT migrated → preserved as if user customization. This is misclassification: legacy factory default is preserved forever, so canonical never replaces it, and runtime injection stays short.

Fresh DB: no keys → seeded canonical — PASS.
Existing DB with old short header-present factory (header but lack hardened marker) → migrated — PASS.
Existing DB with genuine user custom (no header, custom text like "My custom leader instructions...") → preserved — correct.
Existing DB with legacy short prompt (the actual failing case) → misclassified as custom → NOT migrated → BUG.

Same misclassification applies to legacy participant short and legacy system short: both lack header/markers, so not migrated.

Prompt hardening version key `prompt_hardening_version` currently 2, already set in DBs that ran version 2 migration — they will not re-migrate even after fix unless we bump version.

## Other provenance gaps

- `get_prompt_template_with_default` fallback `Ok(match key { ... })` returns canonical if DB empty, but does not override non-empty legacy — same issue.
- `save_agent_brain_config` writes 4 keys but never touches prompt keys unless user saves via save_prompt_template; so legacy persists.
- Frontend `load()` does `invoke('get_prompt_template')` then displays whatever DB returns — user sees short legacy and may think it's canonical.

## Hash provenance — not yet instrumented

No hashes logged at any stage. Cannot detect divergence automatically.

## Browser / Gemini / Kimi / Hackathon / Session quick check (also in pre-audit)

- Browser: two-WebView architecture intact, GENERIC_INIT_SCRIPT static, arena:// IPC, max 2 WebViews, CHROME_USER_AGENT Chrome126, OAuth popup allowed, initialization_script GENERIC_INIT_SCRIPT. No regression.
- Gemini/Claude: as previously, Cloudflare 401/brunhild external, not blocked by our nav handler (allows http/https/about/blob/data), GENERIC_INIT_SCRIPT not accessing challenge iframe (no frame.contentWindow). But need extreme audit per new differential methodology — not yet done in depth.
- Kimi: runtime `https://kimi.ai/` canonical, no kimi.com.
- Hackathon: cross-team DnD state-driven with run patch, responded auto-select with run_id tracking — fixed previous session.
- Session: isDraftSession flag distinguishes New Session draft vs start_session active — fixed.

## Required fixes before post-audit

1. Expand migration detection to catch legacy short prompts (`small expert panel inside Consensus Arena`, `{{AGENTS}}`, `{{PROJECT_BRIEF}}`, hardcoded 7-model list, `For this test run:`, `Finalized blueprint section:` etc.) and bump PROMPT_HARDENING_VERSION to 3 so existing DBs re-migrate legacy short factory content to canonical without overwriting genuine user customs.
2. Add hash/length logging at canonical→stored→retrieved→interpolated→injected stages (dev-safe, SHA256 or length).
3. Add content integrity test markers (Runtime state is authoritative, Route — consult one participant, etc.).
4. Keep two-WebView + GENERIC_INIT_SCRIPT generic, add extreme audit verification for Claude/Gemini.
5. Ensure no new unwrap/expect, no blocking_lock, IPC snake_case.

