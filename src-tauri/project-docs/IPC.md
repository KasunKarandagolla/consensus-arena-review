# Consensus Arena — IPC Contract

## IMPORTANT
This file is the single source of truth for all frontend↔backend communication.
Every event name, command name, and payload field name is authoritative.
Backend and frontend must match this document exactly — no exceptions.
Verified against implementation via full stress audit — all PASS.

---

## Commands (Frontend → Backend)

### Session Management

```typescript
invoke('start_session', {
    project_brief: string,
    session_type: string,        // 'architecture' | 'mvp' | 'api' | 'security' | 'custom'
    agent_ids: string[],         // ['claude', 'deepseek', 'gemini']
    leader_agent_id: string,     // 'claude'
})
// Returns: Promise<void>
// Returns Err if a session is already active (IMP-3 concurrency guard).

invoke('pause_session')
// Returns: Promise<void>

invoke('resume_session')
// Returns: Promise<void>

invoke('abort_session')
// Returns: Promise<void>
// Called by Stop button in frontend
```

### User Interaction

```typescript
invoke('user_input', {
    text: string,
})
// Returns: Promise<void>

invoke('captcha_resolved', {
    agent_id: string,
})
// Returns: Promise<void>

invoke('retry_setup_agent', {
    agent_id: string,
})
// Returns: Promise<void>
// Re-focuses and re-probes one recoverably failed Phase 1 setup agent without
// changing the active session, setup_order, or setup_generation.

invoke('confirm_setup_agent', {
    agent_id: string,
})
// Returns: Promise<void>
// User-confirmed recovery only: marks the current unfinished setup agent as
// primed after the user visibly verifies a send or response. It does not
// submit a prompt or fabricate a browser event.

invoke('provide_manual_model_response', {
    agent_id: string,
    turn_number: number,
    response: string,
})
// Returns: Promise<void>
// User-confirmed active-turn recovery. The backend accepts this only while the
// exact agent_id and turn_number are currently awaiting a response. The text is
// recorded through the normal transcript/blueprint flow with a manual source;
// it does not fabricate an arena:// browser event.

invoke('rate_limit_decision', {
    agent_id: string,
    decision: string,           // 'wait' | 'continue' | 'lighter' | 'skip'
})
// Returns: Promise<void>

invoke('setup_agent_sent', {
    agent_id: string,
})
// Returns: Promise<void>
// Legacy/manual acknowledgement only. It does not substitute for the real
// arena://sent browser signal and cannot advance or complete setup.

invoke('provide_user_answer', {
    answer: string,
})
// Returns: Promise<void>
// Called when user clicks an option in the AskUser popup
// Resumes the paused agent loop with the user's answer
// Returns Err if no pending ask_user question exists
```

### Settings & Configuration

```typescript
invoke('save_agent_brain_config', {
    api_key: string,
    base_url: string,
    model: string,
    system_prompt: string,
})
// Returns: Promise<void>
// Also constructs live AgentBrain instance in backend AppState

invoke('get_agent_brain_config')
// Returns: Promise<string>  (JSON-serialized AgentBrainConfig)

invoke('save_secondary_brain_config', {
    api_key: string,
    base_url: string,
    model: string,
    system_prompt: string,
})
// Returns: Promise<void>

invoke('get_secondary_brain_config')
// Returns: Promise<string>  (JSON-serialized SecondaryBrainConfig — parse it)

invoke('save_fallback_brain_config', {
    api_key: string,
    base_url: string,
    model: string,
})
// Returns: Promise<void>
// No system_prompt field — the fallback always reuses the primary agent
// brain's system_prompt. If a primary agent brain is already configured
// live in AppState, it is updated in place to use the new fallback
// immediately. Passing all three fields empty clears the fallback.

invoke('get_fallback_brain_config')
// Returns: Promise<string>  (JSON-serialized FallbackBrainConfig — parse it)
// FallbackBrainConfig shape: { api_key, base_url, model }

invoke('save_prompt_template', {
    template_name: string,      // 'leader_priming' | 'participant_priming' | 'agent_system'
    content: string,
})
// Returns: Promise<void>

invoke('get_prompt_template', {
    template_name: string,      // 'leader_priming' | 'participant_priming' | 'agent_system'
})
// Returns: Promise<string>
// NOTE: this one is a plain string, NOT JSON-wrapped — do not JSON.parse() it.

invoke('get_diagnostic_snapshot')
// Returns: Promise<string>  (JSON-serialized DiagnosticSnapshot — parse it)
// Secret-free snapshot: app_data_dir, settings.db/memory.db/transcript.db/
// blueprint.db presence, session_active, brain configuration booleans,
// memory health summary, and command timestamp. Never includes keys, prompts,
// cookies, transcripts, model responses, or project brief content.
// Also includes leader_window_exists, nav_window_exists, and
// browser_diagnostics. Each browser record contains agent_id, display_name,
// setup_generation, session_id, selected_leader_id, selected_agent_ids,
// setup_order, intended_url, window_label, window_kind,
// assigned_window_label, assigned_window_kind, is_selected_leader,
// created_at, last_navigation_url, last_ready_at, last_send_detected_at,
// last_response_at, last_error, and current_phase. It also contains
// last_blocker, last_blocker_url_redacted, last_challenge_detected_at,
// resume_attempt_count, last_resume_at, input_found, send_button_found,
// last_send_probe_at, last_user_submit_event_at, last_message_count_seen,
// sent_signal_emitted, expected_agent_id, last_signal_agent_id,
// last_signal_type, last_signal_at, stale_signal_count,
// response_observed_before_send, response_observed_after_injection,
// setup_completion_reason, prompt_injected_at, prompt_injection_error,
// readiness_timeout_ms, readiness_probe_count, input_candidate_count,
// composer_candidate_count, page_state_hint, page_health_hint,
// active_expected_agent_id, active_turn_number, last_active_prompt_injected_at,
// last_active_response_at, active_auto_submit_attempted,
// active_auto_submit_succeeded, active_auto_submit_method,
// active_send_button_enabled_before_submit, active_submit_error, and
// active_submit_at. These active-submit diagnostics never include prompt or
// response text.
// current_phase is queued | creating |
// loading | ready | waiting_user_send | consulting |
// captcha_or_challenge | navigation_error | unshowable_url | error |
// unknown. last_blocker is none | captcha_or_challenge | unsupported_url |
// navigation_error | timeout. URLs are redacted; no keys, prompts, cookies,
// tokens, or model responses are included.
```

### Data Retrieval

```typescript
invoke('get_transcript')
// Returns: Promise<string>  (JSON array of TurnRecord — parse it)

invoke('get_session_list')
// Returns: Promise<string>  (JSON array of SessionSummary — parse it)

invoke('export_blueprint', {
    format: string,              // 'markdown' | 'txt'
    session_id?: string,         // optional
})
// Returns: Promise<string>  (file path of saved file — plain string, do not parse)
// Returns Err if format is not 'markdown' or 'txt' (CRIT-6).
// HIGH-8: if session_id is provided and non-empty, exports that specific
// session's blueprint (e.g. exporting a past session from sidebar
// history). If omitted or empty, falls back to the currently active
// session. Returns Err("No active session") if neither a usable
// session_id nor an active session is available.

invoke('get_agent_health')
// Returns: Promise<string>  (JSON map of agent_id → ModelHealth — parse it)
// ModelHealth shape: { agent_id, is_available, error_count, last_error }
// Returns {} before any session has run.
```

### Phase 1 Memory

```typescript
invoke('get_project_memory', { project_brief: string })
// Returns: Promise<string> (JSON-serialized ProjectMemoryEntry[] — JSON.parse it)

invoke('get_global_memory')
// Returns: Promise<string> (JSON-serialized GlobalMemoryEntry[] — JSON.parse it)

invoke('clear_project_memory', { project_brief: string })
// Returns: Promise<void>

invoke('get_open_questions', { project_brief: string })
// Returns: Promise<string> (JSON-serialized OpenQuestion[] — JSON.parse it)

invoke('get_model_strengths', { project_brief: string })
// Returns: Promise<string> (JSON-serialized ModelStrength[] — JSON.parse it)

invoke('save_project_config', { project_brief: string, content: string })
// Returns: Promise<void>

invoke('get_project_config', { project_brief: string })
// Returns: Promise<string> (plain string — do NOT JSON.parse it)

invoke('get_memory_health')
// Returns: Promise<string> (JSON-serialized MemoryHealth — JSON.parse it)

invoke('repair_memory_index')
// Returns: Promise<void>

invoke('get_patterns', { project_brief: string })
// Returns: Promise<string> (JSON-serialized PatternEntry[] — JSON.parse it)

invoke('export_memory', { destination_path: string })
// Returns: Promise<void>

invoke('restore_memory', { source_path: string })
// Returns: Promise<void>
// Refused while a session is active. Creates a pre-restore backup first.
```

### Session CRUD

```typescript
invoke('delete_session', {
    session_id: string,
})
// Returns: Promise<void>
// Cascades: deletes the session's transcript turns + session row, blueprint
// sections, and saved conversation URLs. Does NOT delete the agent's saved
// cookies (cookies are per-agent login state, not per-session).
// Returns Err if session_id is currently the active session
// (stop it first), or if no session with that id exists.

invoke('rename_session', {
    session_id: string,
    title: string,
})
// Returns: Promise<void>
// Updates the session's project_brief (the field already shown, truncated,
// as the session's title in the sidebar). Returns Err if title is empty
// (after trimming) or if no session with that id exists.

invoke('get_session_details', {
    session_id: string,
})
// Returns: Promise<string>  (JSON-serialized SessionDetails — parse it)
// SessionDetails shape: { id, project_brief, session_type, status,
//   created_at, turn_count, section_count, agent_ids }
// Strictly more detail than a get_session_list row: turn_count and
// section_count are computed from the transcript/blueprint stores,
// agent_ids is the distinct set of agents that actually produced a turn.
// Returns Err if no session with that id exists.
```

### Session Recovery (IMP-7)

```typescript
invoke('get_recovery_state')
// Returns: Promise<string>  (JSON: { available: boolean, session_id: string } — parse it)
// Call on app startup to determine whether to offer recovery.
// available = true only when a previous session was started but never reached Complete.
// Example: { "available": true, "session_id": "abc123..." }

invoke('recover_session', {
    session_id: string,
})
// Returns: Promise<void>
// Re-emits blueprint-section-added for every agreed section of the given session.
// Does NOT re-enter the autonomous session loop — only replays saved blueprint
// sections so the user can see partial output from a previous incomplete session.
// Call after get_recovery_state confirms available = true.
```

---

## Events (Backend → Frontend)

### Session Lifecycle

```typescript
listen('session-status', (event) => {
    const { status, session_id, setup_generation, selected_leader_id, selected_agent_ids, setup_order } = event.payload
    // status: 'setup' | 'requirements' | 'running' | 'paused' | 'complete' | 'ended'
    // During status === 'setup', payload also includes:
    //   session_id: string
    //   setup_generation: number
    //   selected_leader_id: string
    //   selected_agent_ids: string[]
    //   setup_order: string[]   // leader first, then non-leaders in selected-agent order
})

listen('setup-agent-ready', (event) => {
    const { agent_id } = event.payload
    // Model window is open and ready for user to press Send
})

listen('setup-agent-complete', (event) => {
    const { agent_id, conversation_url } = event.payload
    // Browser proof or explicit user confirmation primed this one agent.
    // A manual confirmation records setup_completion_reason:
    // 'user_confirmed_manual'.
})

listen('setup-agent-failed', (event) => {
    const { agent_id, recoverable } = event.payload
    // recoverable is true for browser/login/loading/security readiness failures.
    // Keep setup visible; tell the user to complete the check in the model
    // window and invoke retry_setup_agent. This is not session completion.
})

listen('setup-complete', (event) => {
    // All selected agents emitted real browser send detection and completed
    // priming — autonomous session loop is starting
    // No payload
})

listen('active-turn-state', (event) => {
    const { event, agent_id, turn_number } = event.payload
    // event: 'active_turn_started' | 'active_prompt_injected' |
    //        'active_prompt_submitted' | 'active_submit_failed' |
    //        'active_waiting_for_response' | 'active_response_captured' |
    //        'active_turn_timeout'
    // Safe progress only: contains no prompt or response text.
})
```

### Agent State

```typescript
listen('agent-state-change', (event) => {
    const { agent_id, state, response, tokens } = event.payload
    // state: 'consulting' | 'responded' | 'idle' | 'captcha' | 'rate-limited' | 'error'
    // Used for live status label ONLY — never shown as message in main window
})

listen('browser-diagnostic', (event) => {
    const { agent_id, window_label, phase, url, message, error } = event.payload
    // Secret-free AI WebView lifecycle/status event.
    // phase: 'queued' | 'creating' | 'navigation_started' | 'real_url_loaded' |
    //        'page_script_active' | 'composer_detected' | 'prompt_injected' |
    //        'waiting_user_send' | 'send_detected' | 'response_observed_after_injection' |
    //        'setup_agent_complete' | 'primed' | 'setup_failed_recoverable' |
    //        'active_prompt_injected' | 'active_prompt_submitted' |
    //        'active_submit_failed' | 'active_waiting_for_response' |
    //        'active_response_captured' |
    //        'consulting' | 'captcha_or_challenge' | 'navigation_error' |
    //        'unshowable_url' | 'error' | 'unknown'
    // error: string | null
    // Important errors/challenges may be shown as a toast; loading updates
    // should not produce repetitive toasts. A captcha_or_challenge phase must
    // show the existing CAPTCHA overlay; the user completes verification in
    // the model window and invokes captcha_resolved via Resume.
})
```

### Blueprint (Primary UI Content)

```typescript
listen('blueprint-update', (event) => {
    const { section_id, title, content, status } = event.payload
    // status: 'draft' | 'agreed' | 'negotiation' | 'disputed'
    // Upsert this section in main content area
})

listen('blueprint-section-added', (event) => {
    const { section_id, title, content } = event.payload
    // New finalized section from agent brain Blueprint decision
    // Append to main content area — this is the primary content event
    // Also emitted by recover_session when replaying a previous incomplete session
})

listen('agent_brain_decision_started', (event) => {
    const { iteration, response_length } = event.payload
    // Secret-free decision lifecycle diagnostic; no response text is included.
})

listen('agent_brain_decision_failed', (event) => {
    const { iteration, error, unclassified_count } = event.payload
    // Parsing/API failure diagnostic. error is redacted and contains no prompt text.
})

listen('agent_brain_decision_fallback', (event) => {
    const { kind, target_agent_id, unclassified_count } = event.payload
    // Deterministic fallback selected route, blueprint, or one bounded continue.
})

listen('route_started', (event) => {
    const { iteration, route_target_agent_id } = event.payload
    // Canonical selected participant ID used for the shared nav WebView.
})

listen('blueprint_emitted', (event) => {
    const { iteration, section_title } = event.payload
    // Confirms that a blueprint section was persisted and emitted.
})
```

### Live Status

```typescript
listen('agent-routing', (event) => {
    const { from_model, to_model, reason } = event.payload
    // Update live status label
    // Display: "Routing from [from_model] to [to_model]..."
})

listen('boss-message', (event) => {
    const { text, message_type } = event.payload
    // message_type: 'phase' | 'interruption' | 'status' | 'question'
    // Update live status label with text
})
```

### Conversation Content

```typescript
listen('agent-message', (event) => {
    const { agent_id, role, response, tokens, iteration } = event.payload
    // NOT shown in main window
    // Available in expandable status drawer only
})

listen('requirements-question', (event) => {
    const { question, question_number, total_questions } = event.payload
    // Future use — requirements gathering phase
})

listen('requirements-complete', (event) => {
    const { charter_summary } = event.payload
    // Future use
})
```

### User Interaction Required

```typescript
listen('agent-ask-user', (event) => {
    const { question, options, allow_custom } = event.payload
    // question: string — the question to display
    // options: string[] — 2–4 button labels
    // allow_custom: boolean — if true, also show free-text input
    //
    // Frontend must:
    //   1. Show AskUserPopup immediately
    //   2. Block all other interaction until answered
    //   3. Call provide_user_answer for option click and custom submit
    //      (button or Enter)
    //   4. On Escape or backdrop dismissal, call provide_user_answer
    //      with answer: "Cancelled" so the backend channel does not hang
})
```

### System Events

```typescript
listen('captcha-detected', (event) => {
    const { agent_id } = event.payload
    // Show CAPTCHA overlay — user must resolve in model window
})

listen('rate-limit-reached', (event) => {
    const { agent_id, estimated_reset_mins } = event.payload
    // Show inline rate limit notification with options
    // Also fired automatically by inject_and_wait_with_retry when
    // an agent is found to be in cooldown (IMP-4)
})

listen('session-checkpoint', (event) => {
    const { checkpoint_id, phase } = event.payload
    // Show brief toast: "Progress saved"
})

listen('session-complete', (event) => {
    const { stats } = event.payload
    // stats: { duration_mins, total_turns, sections_agreed, consensus }
    // Show completion state — enable download, revert Stop to inactive
})

listen('memory-updated', (event) => {
    const { memory_type, trigger } = event.payload
    // memory_type: 'session' | 'project' | 'global'
    // trigger: 'routing' | 'route_compare' | 'blueprint' | 'user_answer' | 'session_complete'
})

listen('memory-health-warning', (event) => {
    const { text, fts_needs_repair } = event.payload
    // text: string
    // fts_needs_repair: boolean
})
```

---

## Field Reference

### agent_id values
`'chatgpt'` | `'claude'` | `'gemini'` | `'deepseek'` | `'qwen'` | `'glm'` | `'kimi'`

### session_type values
`'architecture'` | `'mvp'` | `'api'` | `'security'` | `'custom'`

### Blueprint section status values
`'draft'` | `'agreed'` | `'negotiation'` | `'disputed'`

### Agent state values
`'consulting'` | `'responded'` | `'idle'` | `'captcha'` | `'rate-limited'` | `'error'`

### template_name values
`'leader_priming'` | `'participant_priming'` | `'agent_system'`

### rate_limit_decision values
`'wait'` | `'continue'` | `'lighter'` | `'skip'`

---

## Frontend State Shape

```typescript
interface AgentBrainConfig {
    api_key: string
    base_url: string
    model: string
    system_prompt: string
}

interface BlueprintSection {
    id: string
    title: string
    content: string
    status: 'draft' | 'agreed' | 'negotiation' | 'disputed'
}

interface AskUserPayload {
    question: string
    options: string[]
    allow_custom: boolean
}

// IMP-5: per-agent health record returned by get_agent_health
interface ModelHealth {
    agent_id: string
    is_available: boolean
    error_count: number
    last_error: string | null
}

interface AppState {
    sessionStatus: 'idle' | 'setup' | 'priming' | 'running' | 'paused' | 'complete' | 'ended'
    setupProgress: string[]          // agent_ids that have completed priming
    blueprintSections: BlueprintSection[]
    liveStatusText: string           // current status label text
    liveStatusExpanded: boolean      // is the status drawer open
    agentBrainConfig: AgentBrainConfig | null
    selectedSessionId: string | null
    askUserPending: AskUserPayload | null  // non-null when agent-ask-user fires
}
```

---

## Wiring Rules

1. Every `listen()` call must use exact event name strings from this document
2. Every `invoke()` call must use exact command name strings from this document
3. Payload field names must match exactly — case-sensitive
4. No mock data in production — all state from real backend events
5. All `listen()` calls must be cleaned up on component unmount
6. Individual model responses are NOT displayed in main window
7. Main window content comes ONLY from `blueprint-update` and `blueprint-section-added`
8. Live status label content comes from `agent-state-change`, `agent-routing`, `boss-message`
9. `agent-ask-user` must be handled at App root level — not inside a view component
10. `provide_user_answer` must always be called when popup closes — even on dismiss
11. `debug-log` is a development-only event — NOT listed here, NOT in production IPC
12. `get_recovery_state` must be called on app startup before rendering the main view
13. `recover_session` replays blueprint sections only — it does NOT restart the session loop
