# Consensus Arena — Phase 1: Memory System
## Implementation Specification (v10 — Final)

This document is the single source of truth for implementing the memory
system. It contains only what must be built. Read it completely, then
implement exactly what it specifies.

---

## Prerequisite

Backend must be at the state described in the project's Current State
section (D-035–D-042 batch, Task 1-12 batch, `db_helpers.rs` with
`spawn_blocking`-wrapped DB access, 14 AppState fields, 26 commands, all
registered). `cargo check` and `npm run build` must both pass with 0
errors before starting this work.

---

## What This Delivers

A six-table SQLite memory system that:
- Persists project decisions, session facts, open questions, per-model
  reliability signals, and reusable patterns across sessions
- Injects a bounded, prioritized memory context into the agent brain at
  every decision point
- Tracks provenance (who/what asserted a fact, how confident it is)
- Hard-pins the user's Project Context so it is never silently dropped
- Supports export, restore (with automatic pre-restore backup), health
  check, and search-index repair
- Records model-reliability signals for both `Route` and `RouteCompare`
  decisions
- Adds five forward-compatible columns for Phase 2 (Skills) at zero
  current cost

---

## 1. Cargo.toml (MODIFY)

```toml
rusqlite = { version = "0.31", features = ["bundled", "backup"] }
```

If `@tauri-apps/plugin-dialog` is not already a dependency, add:

```toml
tauri-plugin-dialog = "2"
```

and in `package.json`:

```json
"@tauri-apps/plugin-dialog": "^2"
```

Check the current `Cargo.toml`/`package.json` first — only add what is
actually missing.

---

## 2. Database Schema

All tables live in `app_data_dir/memory.db`.

### PRAGMAs — run immediately after opening the connection

```sql
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA temp_store = memory;
PRAGMA busy_timeout = 5000;
```

### Schema version

```rust
let version: i64 = conn
    .pragma_query_value(None, "user_version", |r| r.get(0))
    .unwrap_or(0);
if version < 1 {
    conn.pragma_update(None, "user_version", 1)?;
}
```

### Table: session_memory

```sql
CREATE TABLE IF NOT EXISTS session_memory (
    id            TEXT PRIMARY KEY,
    session_id    TEXT NOT NULL,
    project_brief TEXT NOT NULL,
    category      TEXT NOT NULL,
    content       TEXT NOT NULL,
    skill_name    TEXT,
    source_agent  TEXT NOT NULL DEFAULT 'leader',
    source_type   TEXT NOT NULL DEFAULT 'llm',
    archived      INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL,
    CHECK (source_type IN ('llm', 'user', 'tool', 'opencode', 'confirmed', 'observed', 'inferred', 'imported'))
);

CREATE INDEX IF NOT EXISTS idx_session_memory_session ON session_memory (session_id);
CREATE INDEX IF NOT EXISTS idx_session_memory_active ON session_memory (project_brief, archived);
```

### Table: project_memory

```sql
CREATE TABLE IF NOT EXISTS project_memory (
    id                TEXT PRIMARY KEY,
    project_brief     TEXT NOT NULL,
    category          TEXT NOT NULL,
    content           TEXT NOT NULL,
    trigger_desc      TEXT,
    importance        TEXT NOT NULL DEFAULT 'normal',
    superseded_by     TEXT,
    hard_pinned       INTEGER NOT NULL DEFAULT 0,
    skill_name        TEXT,
    source_agent      TEXT NOT NULL DEFAULT 'leader',
    source_type       TEXT NOT NULL DEFAULT 'llm',
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    last_confirmed_at INTEGER NOT NULL,
    last_accessed_at  INTEGER NOT NULL DEFAULT 0,
    access_count      INTEGER NOT NULL DEFAULT 0,
    mention_count     INTEGER NOT NULL DEFAULT 1,

    FOREIGN KEY (superseded_by) REFERENCES project_memory(id) ON DELETE SET NULL,
    CHECK (importance IN ('high', 'normal', 'low', 'superseded')),
    CHECK (category IN (
        'project_config', 'decision', 'deferred', 'rejected', 'observation',
        'pattern', 'user_preference', 'routing', 'investigated', 'completed',
        'learned', 'next_steps'
    )),
    CHECK (source_type IN ('llm', 'user', 'tool', 'opencode', 'confirmed', 'observed', 'inferred', 'imported'))
);

CREATE INDEX IF NOT EXISTS idx_project_memory_brief ON project_memory (project_brief);
CREATE INDEX IF NOT EXISTS idx_project_memory_superseded ON project_memory (superseded_by);
CREATE INDEX IF NOT EXISTS idx_project_memory_rank
    ON project_memory (project_brief, hard_pinned, importance, last_confirmed_at, last_accessed_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_project_memory_unique_content
    ON project_memory (project_brief, category, content);
```

### Table: project_memory_fts (external-content FTS5)

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS project_memory_fts
USING fts5(
    project_brief, category, content, trigger_desc,
    content=project_memory, content_rowid=rowid
);

INSERT INTO project_memory_fts(project_memory_fts, rank)
VALUES('rank', 'bm25(0.0, 0.2, 2.0, 1.5)');

CREATE TRIGGER IF NOT EXISTS project_memory_ai
AFTER INSERT ON project_memory BEGIN
    INSERT INTO project_memory_fts(rowid, project_brief, category, content, trigger_desc)
    VALUES (new.rowid, new.project_brief, new.category, new.content, new.trigger_desc);
END;

CREATE TRIGGER IF NOT EXISTS project_memory_ad
AFTER DELETE ON project_memory BEGIN
    INSERT INTO project_memory_fts(project_memory_fts, rowid, project_brief, category, content, trigger_desc)
    VALUES ('delete', old.rowid, old.project_brief, old.category, old.content, old.trigger_desc);
END;

CREATE TRIGGER IF NOT EXISTS project_memory_au
AFTER UPDATE ON project_memory BEGIN
    INSERT INTO project_memory_fts(project_memory_fts, rowid, project_brief, category, content, trigger_desc)
    VALUES ('delete', old.rowid, old.project_brief, old.category, old.content, old.trigger_desc);
    INSERT INTO project_memory_fts(rowid, project_brief, category, content, trigger_desc)
    VALUES (new.rowid, new.project_brief, new.category, new.content, new.trigger_desc);
END;
```

### Table: global_memory

```sql
CREATE TABLE IF NOT EXISTS global_memory (
    id            TEXT PRIMARY KEY,
    category      TEXT NOT NULL,
    content       TEXT NOT NULL,
    importance    TEXT NOT NULL DEFAULT 'normal',
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    mention_count INTEGER NOT NULL DEFAULT 1,
    CHECK (importance IN ('high', 'normal', 'low'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_global_memory_unique ON global_memory (category, content);
```

### Table: open_questions

```sql
CREATE TABLE IF NOT EXISTS open_questions (
    id               TEXT PRIMARY KEY,
    session_id       TEXT NOT NULL,
    project_brief    TEXT NOT NULL,
    skill_name       TEXT,
    source_agent     TEXT NOT NULL DEFAULT 'leader',
    question         TEXT NOT NULL,
    question_key     TEXT NOT NULL,
    raised_iteration INTEGER NOT NULL DEFAULT 0,
    resolved         INTEGER NOT NULL DEFAULT 0,
    resolution       TEXT,
    created_at       INTEGER NOT NULL,
    resolved_at      INTEGER
);
CREATE INDEX IF NOT EXISTS idx_open_questions_brief ON open_questions (project_brief, resolved);
CREATE UNIQUE INDEX IF NOT EXISTS idx_open_questions_unique_active
    ON open_questions (project_brief, question_key, resolved);
```

### Table: model_reliability

```sql
CREATE TABLE IF NOT EXISTS model_reliability (
    id            TEXT PRIMARY KEY,
    project_brief TEXT NOT NULL,
    model_id      TEXT NOT NULL,
    skill_name    TEXT,
    topic         TEXT NOT NULL,
    adopted       INTEGER NOT NULL,
    session_id    TEXT NOT NULL,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_model_reliability_brief ON model_reliability (project_brief, model_id, topic);
CREATE INDEX IF NOT EXISTS idx_model_reliability_strength ON model_reliability (project_brief, topic, adopted);
```

### Table: pattern_memory

```sql
CREATE TABLE IF NOT EXISTS pattern_memory (
    id                TEXT PRIMARY KEY,
    project_brief     TEXT NOT NULL,
    pattern_condition TEXT NOT NULL,
    pattern_action    TEXT NOT NULL,
    pattern_outcome   TEXT NOT NULL,
    confidence        INTEGER NOT NULL DEFAULT 1,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pattern_memory_brief ON pattern_memory (project_brief);
CREATE UNIQUE INDEX IF NOT EXISTS idx_pattern_memory_unique
    ON pattern_memory (project_brief, pattern_condition, pattern_action);
```

---

## 3. Category and Source Type Values

**Category values:** `project_config`, `decision`, `deferred`, `rejected`,
`observation`, `pattern`, `user_preference`, `routing`, `investigated`,
`completed`, `learned`, `next_steps`.

**Source type values, in retrieval priority order (highest first):**
`confirmed` (direct user answer/confirmation) → `observed` (directly
observed from app/session) → `imported` (from backup) → `user`
(user-authored, not necessarily confirmed) → `llm` (leader/brain
generated) → `inferred` (system-inferred, lower confidence) → `tool` /
`opencode` (forward-compatible only — Phase 1 never writes these).

---

## 4. `src-tauri/src/memory_store.rs` (NEW FILE — complete implementation)

### Structs

```rust
use serde::{Serialize, Deserialize};
use crate::errors::AgentError;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub session_id: String,
    pub project_brief: String,
    pub category: String,
    pub content: String,
    pub skill_name: Option<String>,
    pub source_agent: String,
    pub source_type: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectMemoryEntry {
    pub id: String,
    pub project_brief: String,
    pub category: String,
    pub content: String,
    pub trigger_desc: Option<String>,
    pub importance: String,
    pub superseded_by: Option<String>,
    pub hard_pinned: bool,
    pub skill_name: Option<String>,
    pub source_agent: String,
    pub source_type: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_confirmed_at: i64,
    pub last_accessed_at: i64,
    pub access_count: i64,
    pub mention_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMemoryEntry {
    pub id: String,
    pub category: String,
    pub content: String,
    pub importance: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub mention_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenQuestion {
    pub id: String,
    pub session_id: String,
    pub project_brief: String,
    pub skill_name: Option<String>,
    pub source_agent: String,
    pub question: String,
    pub question_key: String,
    pub raised_iteration: i64,
    pub resolved: bool,
    pub resolution: Option<String>,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStrength {
    pub model_id: String,
    pub topic: String,
    pub adoption_rate: f64,
    pub total_count: i64,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternEntry {
    pub id: String,
    pub project_brief: String,
    pub pattern_condition: String,
    pub pattern_action: String,
    pub pattern_outcome: String,
    pub confidence: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryHealth {
    pub is_healthy: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub table_counts: HashMap<String, i64>,
    pub fts_needs_repair: bool,
}

pub enum MemoryReadOutcome<T> {
    Found(T),
    Empty,
    Failed(String),
}

pub struct MemoryStore {
    conn: rusqlite::Connection,
}
```

### Constructors

```rust
pub fn new(db_path: &str) -> Result<Self, AgentError>
```
Opens the connection, applies the PRAGMAs, creates all six tables plus
the FTS5 virtual table, configures the FTS5 rank weighting, creates the
three sync triggers, sets `user_version`. Maps all `rusqlite` errors to
`AgentError::DatabaseError(e.to_string())`.

```rust
pub fn new_empty() -> Self
```
Opens `:memory:` and runs the identical schema setup. Used only when
persistent initialization fails.

### Transaction helper

```rust
pub fn with_transaction<F, T>(&mut self, f: F) -> Result<T, AgentError>
where
    F: FnOnce(&rusqlite::Transaction) -> Result<T, AgentError>,
{
    let tx = self.conn.transaction()
        .map_err(|e| AgentError::DatabaseError(e.to_string()))?;
    let result = f(&tx)?;
    tx.commit().map_err(|e| AgentError::DatabaseError(e.to_string()))?;
    Ok(result)
}
```
Only synchronous SQL runs inside the closure. Never call `MemoryStore`'s
own `&mut self` methods from inside the closure — that would recursively
touch a connection already borrowed by the transaction.

### Safe string helpers

```rust
pub fn safe_prefix(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

pub fn normalize_question_key(question: &str) -> String {
    question
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(40)
        .collect()
}
```
Both are `pub`. Never slice strings by byte index for any user-visible or
stored text — always use `.chars()` to avoid panics on multi-byte UTF-8
boundaries.

### Topic detection

```rust
pub fn detect_topic(prompt: &str) -> &'static str {
    let lower = prompt.to_lowercase();
    if lower.contains("database") || lower.contains("schema") || lower.contains("sql")
        || lower.contains("postgres") || lower.contains("mysql") || lower.contains("migration") {
        return "database";
    }
    if lower.contains("auth") || lower.contains("login") || lower.contains("jwt")
        || lower.contains("password") || lower.contains("token") || lower.contains("session") {
        return "auth";
    }
    if lower.contains("api") || lower.contains("endpoint") || lower.contains("rest")
        || lower.contains("graphql") || lower.contains("route") || lower.contains("request") {
        return "api";
    }
    if lower.contains("security") || lower.contains("vulnerability") || lower.contains("owasp")
        || lower.contains("encrypt") || lower.contains("attack") || lower.contains("xss") {
        return "security";
    }
    if lower.contains("architect") || lower.contains("design") || lower.contains("system")
        || lower.contains("scalab") || lower.contains("infrastructure") {
        return "architecture";
    }
    if lower.contains("test") || lower.contains("ci") || lower.contains("deploy")
        || lower.contains("pipeline") || lower.contains("quality") {
        return "devops";
    }
    "general"
}
```

### Session memory

```rust
pub fn add_session_fact(
    &mut self, session_id: &str, project_brief: &str, category: &str, content: &str,
    skill_name: Option<&str>, source_agent: &str, source_type: &str,
) -> Result<(), AgentError>
```
`INSERT` into `session_memory`. `id = uuid::Uuid::new_v4().to_string()`,
`created_at = chrono::Utc::now().timestamp()`.

```rust
pub fn get_session_facts(&self, session_id: &str) -> Result<Vec<MemoryEntry>, AgentError>
```
`SELECT * FROM session_memory WHERE session_id = ? AND archived = 0 ORDER BY created_at ASC`

```rust
pub fn archive_old_session_facts(
    &mut self, project_brief: &str, current_session_id: &str,
) -> Result<usize, AgentError>
```
`UPDATE session_memory SET archived = 1 WHERE project_brief = ? AND
session_id != ? AND archived = 0`. Called at the start of every new
session. Returns the number of rows archived.

### Project memory

```rust
pub fn add_project_memory_with_source(
    &mut self, project_brief: &str, category: &str, content: &str,
    trigger_desc: Option<&str>, skill_name: Option<&str>,
    source_agent: &str, source_type: &str,
) -> Result<(), AgentError>
```
Single atomic upsert:
```sql
INSERT INTO project_memory
    (id, project_brief, category, content, trigger_desc, importance,
     hard_pinned, skill_name, source_agent, source_type,
     created_at, updated_at, last_confirmed_at, mention_count)
VALUES (?1, ?2, ?3, ?4, ?5, 'normal', 0, ?6, ?7, ?8, ?9, ?9, ?9, 1)
ON CONFLICT(project_brief, category, content) DO UPDATE SET
    mention_count = mention_count + 1,
    updated_at = ?9,
    last_confirmed_at = ?9,
    trigger_desc = COALESCE(project_memory.trigger_desc, excluded.trigger_desc),
    importance = CASE
        WHEN project_memory.mention_count + 1 >= 3 THEN 'high'
        ELSE project_memory.importance
    END
```
Never writes rows where `importance = 'superseded'`.

```rust
pub fn add_project_memory(
    &mut self, project_brief: &str, category: &str, content: &str,
    trigger_desc: Option<&str>, skill_name: Option<&str>,
) -> Result<(), AgentError> {
    self.add_project_memory_with_source(
        project_brief, category, content, trigger_desc, skill_name, "leader", "llm",
    )
}
```
Thin wrapper — the default provenance for every leader/brain-originated
write.

```rust
pub fn save_project_config(&mut self, project_brief: &str, content: &str) -> Result<(), AgentError>
```
Caps `content` to 2000 characters using `safe_prefix`. Inside
`with_transaction`:
```sql
DELETE FROM project_memory WHERE project_brief = ?1 AND category = 'project_config';
INSERT INTO project_memory
    (id, project_brief, category, content, trigger_desc, importance,
     hard_pinned, skill_name, source_agent, source_type,
     created_at, updated_at, last_confirmed_at, mention_count)
VALUES (?2, ?1, 'project_config', ?3, NULL, 'high', 1, NULL, 'user', 'confirmed', ?4, ?4, ?4, 1)
```
Delete-then-insert inside one transaction — never a moment where config
is absent. There is exactly one active `project_config` row per project
brief.

```rust
pub fn get_project_config(&self, project_brief: &str) -> Result<String, AgentError>
```
`SELECT content FROM project_memory WHERE project_brief = ? AND category
= 'project_config' ORDER BY created_at DESC LIMIT 1`. Returns empty
string if no row exists.

```rust
fn get_project_memory_peek(&self, project_brief: &str) -> Result<Vec<ProjectMemoryEntry>, AgentError>
```
Non-mutating. Never bumps access stats.
```sql
SELECT * FROM project_memory
WHERE project_brief = ?1 AND importance != 'superseded'
ORDER BY
    hard_pinned DESC,
    CASE source_type
        WHEN 'confirmed' THEN 6 WHEN 'observed' THEN 5 WHEN 'imported' THEN 4
        WHEN 'user' THEN 3 WHEN 'llm' THEN 2 WHEN 'inferred' THEN 1 ELSE 0
    END DESC,
    CASE importance WHEN 'high' THEN 3 WHEN 'normal' THEN 2 ELSE 1 END DESC,
    mention_count DESC,
    MAX(last_confirmed_at, last_accessed_at) DESC,
    created_at ASC
```

```rust
pub fn get_project_memory(&mut self, project_brief: &str) -> Result<Vec<ProjectMemoryEntry>, AgentError>
```
Calls `get_project_memory_peek`, then for every returned row bumps
`last_accessed_at = now()` and `access_count = access_count + 1` in a
single `UPDATE ... WHERE id IN (...)` statement. Best-effort — a failed
bump never fails the read; log and continue.

```rust
pub fn get_project_memory_checked(&self, project_brief: &str) -> MemoryReadOutcome<Vec<ProjectMemoryEntry>> {
    match self.get_project_memory_peek(project_brief) {
        Ok(rows) if rows.is_empty() => MemoryReadOutcome::Empty,
        Ok(rows) => MemoryReadOutcome::Found(rows),
        Err(e) => MemoryReadOutcome::Failed(e.to_string()),
    }
}
```
`&self`, not `&mut self` — uses the peek variant, so it never bumps
access stats. This is purely diagnostic (for the log-branching decision
at session start) and must never be counted as a real read.

```rust
pub fn clear_project_memory(&mut self, project_brief: &str) -> Result<(), AgentError>
```
Inside `with_transaction`: `DELETE FROM project_memory, session_memory,
open_questions, model_reliability, pattern_memory WHERE project_brief =
?` (five deletes, one transaction).

### Full-text search

```rust
pub fn search_project_memory(
    &mut self, project_brief: &str, query: &str, limit: usize,
) -> Result<Vec<ProjectMemoryEntry>, AgentError>
```
```sql
SELECT pm.* FROM project_memory pm
JOIN project_memory_fts fts ON pm.rowid = fts.rowid
WHERE pm.project_brief = ?1
  AND project_memory_fts MATCH ?2
  AND pm.importance != 'superseded'
ORDER BY rank
LIMIT ?3
```
Filter on `pm.project_brief` (the real table) — never `fts.project_brief`
(the virtual table's tokenized copy; filtering there would fuzzy-match
instead of exact-match). `ORDER BY rank` ascending is correct as written
— never add `DESC`. Any query error or no-match returns `Ok(vec![])`,
never `Err`. Bumps access stats on returned rows, same as
`get_project_memory`.

```rust
pub fn repair_fts_index(&mut self) -> Result<(), AgentError>
```
```sql
INSERT INTO project_memory_fts(project_memory_fts) VALUES('rebuild');
```

### Open questions

```rust
pub fn add_open_question(
    &mut self, session_id: &str, project_brief: &str, question: &str, raised_iteration: u32,
) -> Result<(), AgentError>
```
Compute `question_key = normalize_question_key(question)`. Insert with
`resolved = 0`. The unique index on `(project_brief, question_key,
resolved)` prevents duplicate active questions — use `INSERT OR IGNORE`
or check the conflict explicitly.

```rust
pub fn resolve_question(
    &mut self, session_id: &str, question_prefix: &str, resolution: &str,
) -> Result<(), AgentError>
```
`UPDATE open_questions SET resolved = 1, resolution = ?, resolved_at = ?
WHERE session_id = ? AND question_key = ? AND resolved = 0` — compute
`question_key` from `question_prefix` the same way as insertion.

```rust
pub fn get_open_questions(&self, project_brief: &str) -> Result<Vec<OpenQuestion>, AgentError>
```
`SELECT * WHERE project_brief = ? AND resolved = 0 ORDER BY created_at ASC`

### Model reliability

```rust
pub fn record_model_response(
    &mut self, project_brief: &str, model_id: &str, topic: &str,
    adopted: bool, session_id: &str,
) -> Result<(), AgentError>
```
`INSERT` into `model_reliability`.

```rust
pub fn get_model_strengths(&self, project_brief: &str) -> Result<Vec<ModelStrength>, AgentError>
```
`GROUP BY model_id, topic; HAVING COUNT(*) >= 2`. `adoption_rate =
SUM(adopted) / COUNT(*)`. Label: `>= 0.7` "strong", `0.4..0.7`
"moderate", `< 0.4` "weak".

### Pattern memory

```rust
pub fn add_pattern(
    &mut self, project_brief: &str, pattern_condition: &str,
    pattern_action: &str, pattern_outcome: &str,
) -> Result<(), AgentError>
```
Upsert keyed on `(project_brief, pattern_condition, pattern_action)`
matching the unique index: increment `confidence`, update
`pattern_outcome` and `updated_at` on conflict; insert with
`confidence = 1` otherwise. Non-fatal on failure.

```rust
pub fn get_patterns(&self, project_brief: &str) -> Result<Vec<PatternEntry>, AgentError>
```
`ORDER BY confidence DESC, updated_at DESC LIMIT 10`

### Blueprint finalization (single method — avoids scattering writes across response_router.rs)

```rust
pub fn record_blueprint_finalized(
    &mut self,
    project_brief: &str,
    section_title: &str,
    models_consulted: &[String],
    iterations_since_last_section: u32,
) -> Result<(), AgentError> {
    let topic = detect_topic(section_title).to_string();
    let models_joined = models_consulted.join(", ");
    let title_lower = section_title.to_lowercase();

    self.with_transaction(|tx| {
        let now = chrono::Utc::now().timestamp();
        tx.execute(
            "INSERT INTO project_memory
             (id, project_brief, category, content, importance,
              hard_pinned, source_agent, source_type,
              created_at, updated_at, last_confirmed_at, mention_count)
             VALUES (?1, ?2, 'decision', ?3, 'normal', 0, 'leader', 'llm', ?4, ?4, ?4, 1)
             ON CONFLICT(project_brief, category, content) DO UPDATE SET
                 mention_count = mention_count + 1, updated_at = ?4, last_confirmed_at = ?4",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(), project_brief,
                format!("Blueprint section finalised: {}", section_title), now
            ],
        ).map_err(|e| AgentError::DatabaseError(e.to_string()))?;

        tx.execute(
            "INSERT INTO pattern_memory
             (id, project_brief, pattern_condition, pattern_action, pattern_outcome,
              confidence, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)
             ON CONFLICT(project_brief, pattern_condition, pattern_action) DO UPDATE SET
                 confidence = confidence + 1, pattern_outcome = excluded.pattern_outcome, updated_at = ?6",
            rusqlite::params![
                uuid::Uuid::new_v4().to_string(), project_brief,
                format!("{} section", topic),
                format!("consulted: {}", models_joined),
                format!("finalised in {} iterations", iterations_since_last_section),
                now
            ],
        ).map_err(|e| AgentError::DatabaseError(e.to_string()))?;

        let words: Vec<&str> = title_lower.split_whitespace().take(3).collect();
        for word in words {
            if word.len() > 4 {
                let key = normalize_question_key(word);
                tx.execute(
                    "UPDATE open_questions SET resolved = 1, resolution = ?1, resolved_at = ?2
                     WHERE project_brief = ?3 AND question_key = ?4 AND resolved = 0",
                    rusqlite::params![
                        format!("Resolved via blueprint section: {}", section_title),
                        now, project_brief, key
                    ],
                ).ok();
            }
        }
        Ok(())
    })
}
```
All three writes (project decision, pattern, question resolution) commit
together in one transaction. Non-fatal at the call site in
`response_router.rs` — log on failure, never abort the session loop.

### Session summary

```rust
pub struct SessionSummaryData {
    pub investigated: Vec<String>,
    pub completed: Vec<String>,
    pub learned: Vec<String>,
    pub next_steps: Vec<String>,
}

pub fn write_session_completion_memory(
    &mut self, session_id: &str, project_brief: &str,
    summary: &SessionSummaryData, open_questions: &[OpenQuestion],
    sections_finalized: u32, total_iterations: u32,
) -> Result<(), AgentError>
```
Inside `with_transaction`: writes one `project_memory` row per
non-empty summary section (`category` = `"investigated"` /
`"completed"` / `"learned"` / `"next_steps"`, `content` = the Vec joined
with `"; "`); writes one `project_memory` row per unresolved question in
`open_questions` (`category = "deferred"`, `content = "Unresolved at
session end: {question}"`); writes one `global_memory` row if
`sections_finalized >= 3` (`"Session complete: {n} sections agreed in
{m} iterations"`).

### Decay

```rust
pub fn decay_stale_importance(&mut self, project_brief: &str) -> Result<usize, AgentError>
```
```sql
UPDATE project_memory
SET importance = 'low', updated_at = ?1
WHERE project_brief = ?2
  AND importance = 'normal'
  AND hard_pinned = 0
  AND MAX(last_confirmed_at, last_accessed_at) < ?3
```
`?3` = `now - (30 * 24 * 60 * 60)`, computed once in Rust and passed as
a parameter. Never touches `high`, `superseded`, or `hard_pinned = 1`
rows — the `hard_pinned = 0` clause is explicit even though hard-pinned
rows are typically also `high`, to guard against future logic changes.

### Health check

```rust
pub fn check_health(&self) -> MemoryHealth
```
Never returns `Err`. Runs `PRAGMA integrity_check` (expect `"ok"`;
anything else is an issue). Counts rows in all six tables into
`table_counts` (a failed count is `-1` plus an issue). Compares
`COUNT(*)` from `project_memory` vs `project_memory_fts` — a mismatch
sets `fts_needs_repair = true` and adds a warning, but does not set
`is_healthy = false` on its own. `is_healthy = issues.is_empty()`.

### Export / restore

```rust
pub fn export_to(&self, destination_path: &str) -> Result<(), AgentError> {
    let mut dst = rusqlite::Connection::open(destination_path)
        .map_err(|e| AgentError::DatabaseError(e.to_string()))?;
    let backup = rusqlite::backup::Backup::new(&self.conn, &mut dst)
        .map_err(|e| AgentError::DatabaseError(e.to_string()))?;
    backup.run_to_completion(5, std::time::Duration::from_millis(250), None)
        .map_err(|e| AgentError::DatabaseError(e.to_string()))
}

pub fn restore_from(&mut self, source_path: &str) -> Result<(), AgentError> {
    let src = rusqlite::Connection::open(source_path)
        .map_err(|e| AgentError::DatabaseError(e.to_string()))?;
    let version: i64 = src.pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|e| AgentError::DatabaseError(format!("source file doesn't look like a memory.db: {e}")))?;
    if version < 1 {
        return Err(AgentError::DatabaseError(
            "source file is missing the expected schema version stamp".to_string()
        ));
    }
    let backup = rusqlite::backup::Backup::new(&src, &mut self.conn)
        .map_err(|e| AgentError::DatabaseError(e.to_string()))?;
    backup.run_to_completion(5, std::time::Duration::from_millis(250), None)
        .map_err(|e| AgentError::DatabaseError(e.to_string()))
}
```
`restore_from` validates the source's `user_version` before overwriting
live data. Whether a session is active, and creating the automatic
pre-restore backup, are handled at the Tauri command layer (Section 7
below) — `MemoryStore` itself has no knowledge of `AppState` or session
status.

### Shutdown

```rust
pub fn commit_pending_state(&mut self) -> Result<(), AgentError> {
    self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|e| AgentError::DatabaseError(e.to_string()))
}
```

### Build memory context

```rust
pub fn build_memory_context(
    &mut self,
    session_id: &str,
    project_brief: &str,
    session_type: Option<&str>,
    recent_leader_text: Option<&str>,
) -> Result<String, AgentError>
```

No separate `project_config` parameter — Project Config is read as a
hard-pinned `project_memory` row via the normal query path, not passed
in from outside.

**Budget:** 4000 characters, soft cap. Hard-pinned rows are the one
exception — always included even if they alone exceed the cap (log a
warning, include anyway).

**Output format** — plain text, section headers:
```text
=== PROJECT CONFIG — HARD PINNED ===
...

=== HIGH-CONFIDENCE PROJECT MEMORY ===
...

=== RELEVANT RECALLED MEMORY ===
...

=== OPEN QUESTIONS ===
...

=== MODEL STRENGTH HINTS ===
...
```

**Build order:**
1. Every row where `hard_pinned = 1` (from `get_project_memory`), led by
   any `category = 'project_config'` row.
2. High-importance rows, in the priority order already established by
   `get_project_memory`'s `ORDER BY`, until the running total approaches
   the cap.
3. If room remains past 2500 characters used: call
   `search_project_memory` with a query built from `session_type` (if
   given) or `detect_topic(recent_leader_text)` (fallback), limit 5.
4. Up to 5 open questions (`get_open_questions`).
5. Up to 5 model-strength hints (`get_model_strengths`, only rows with
   `label != "weak"`).

**Deduplication:** track included row ids and a normalized-content
signature in a `HashSet`; skip anything already present so step 2's fill
and step 3's search never repeat the same fact, and Project Config is
never duplicated between step 1 and step 2.

Do not switch this output to XML in Phase 1.

---

## 5. `src-tauri/src/orchestrator.rs` (MODIFY)

Add to `AppState`:
```rust
pub memory_store: Arc<std::sync::Mutex<MemoryStore>>,
pub last_memory_health: MemoryHealth,
```

Inside `AppState::new(data_dir: &str)` — do not change this signature,
do not split it into separate settings/memory path parameters:
```rust
let memory_db_path = PathBuf::from(data_dir).join("memory.db");
let memory_store = MemoryStore::new(memory_db_path.to_string_lossy().as_ref())
    .unwrap_or_else(|e| {
        eprintln!("[MEMORY] Init failed: {e}. Using in-memory fallback.");
        MemoryStore::new_empty()
    });
let last_memory_health = memory_store.check_health();
```
Add `memory_store: Arc::new(std::sync::Mutex::new(memory_store)),
last_memory_health,` to the struct literal.

---

## 6. `src-tauri/src/main.rs` (MODIFY)

Add `mod memory_store;`.

Inside `.setup()`, after `AppState::new(...)` and before `.manage(...)`:
```rust
if !app_state.last_memory_health.is_healthy {
    let issues = app_state.last_memory_health.issues.join("; ");
    eprintln!("[MEMORY] Health check found issues: {issues}");
    let handle = app.handle();
    handle.emit("boss-message", serde_json::json!({
        "text": "Memory database issue detected. Some session history may be unavailable.",
        "message_type": "status"
    })).ok();
}
```

If `@tauri-apps/plugin-dialog` was added in Section 1, register it:
```rust
.plugin(tauri_plugin_dialog::init())
```

Add an app-exit shutdown hook (best-effort, never blocking, never
panicking):
```rust
if let tauri::RunEvent::ExitRequested { .. } = event {
    if let Ok(mut mem) = app_state.memory_store.lock() {
        let _ = mem.commit_pending_state();
    }
}
```
Use whichever exit-event mechanism this codebase's existing shutdown
handling already uses, if any; add this minimally if none exists yet.

Register these 12 commands in `generate_handler!`:
```
get_project_memory, get_global_memory, clear_project_memory,
get_open_questions, get_model_strengths, save_project_config,
get_project_config, get_memory_health, repair_memory_index,
get_patterns, export_memory, restore_memory,
```

---

## 7. `src-tauri/src/commands.rs` (MODIFY)

All memory DB operations from these `async fn` commands go through the
existing `db_helpers::run_blocking()` pattern already used for
`TranscriptStore`/`BlueprintStore`/`SessionVault` — do not introduce a
different locking convention for this store.

```rust
#[tauri::command]
pub async fn get_project_memory(state: tauri::State<'_, AppState>, project_brief: String) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let entries = crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.get_project_memory(&project_brief)
    }).await.map_err(|e| e.to_string())?;
    serde_json::to_string(&entries).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_global_memory(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let entries = crate::db_helpers::run_blocking(move || {
        let mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.get_global_memory()
    }).await.map_err(|e| e.to_string())?;
    serde_json::to_string(&entries).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn clear_project_memory(state: tauri::State<'_, AppState>, project_brief: String) -> Result<(), String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.clear_project_memory(&project_brief)
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_open_questions(state: tauri::State<'_, AppState>, project_brief: String) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let questions = crate::db_helpers::run_blocking(move || {
        let mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.get_open_questions(&project_brief)
    }).await.map_err(|e| e.to_string())?;
    serde_json::to_string(&questions).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_model_strengths(state: tauri::State<'_, AppState>, project_brief: String) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let strengths = crate::db_helpers::run_blocking(move || {
        let mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.get_model_strengths(&project_brief)
    }).await.map_err(|e| e.to_string())?;
    serde_json::to_string(&strengths).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_project_config(state: tauri::State<'_, AppState>, project_brief: String, content: String) -> Result<(), String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.save_project_config(&project_brief, &content)
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_project_config(state: tauri::State<'_, AppState>, project_brief: String) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.get_project_config(&project_brief)
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_memory_health(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let health = crate::db_helpers::run_blocking(move || {
        let mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        Ok(mem.check_health())
    }).await.map_err(|e| e.to_string())?;
    serde_json::to_string(&health).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn repair_memory_index(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.repair_fts_index()
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_patterns(state: tauri::State<'_, AppState>, project_brief: String) -> Result<String, String> {
    let memory_store = state.memory_store.clone();
    let patterns = crate::db_helpers::run_blocking(move || {
        let mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.get_patterns(&project_brief)
    }).await.map_err(|e| e.to_string())?;
    serde_json::to_string(&patterns).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_memory(state: tauri::State<'_, AppState>, destination_path: String) -> Result<(), String> {
    let memory_store = state.memory_store.clone();
    crate::db_helpers::run_blocking(move || {
        let mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.export_to(&destination_path)
    }).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn restore_memory(state: tauri::State<'_, AppState>, source_path: String) -> Result<(), String> {
    if state.session_active.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Cannot restore memory while a session is active. Stop the session first.".to_string());
    }
    let memory_store = state.memory_store.clone();
    // Resolve app_data_dir the same way this file already does elsewhere,
    // then ensure app_data_dir/memory_backups/ exists (create if missing).
    let backup_dir = /* app_data_dir + "/memory_backups" */;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let pre_restore_path = format!("{}/pre_restore_{}.db", backup_dir, timestamp);

    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.export_to(&pre_restore_path)?;
        mem.restore_from(&source_path)?;
        let health = mem.check_health();
        if !health.is_healthy {
            return Err(AgentError::DatabaseError(format!(
                "Restore completed but health check failed: {}. Pre-restore backup saved at {}",
                health.issues.join("; "), pre_restore_path
            )));
        }
        Ok(())
    }).await.map_err(|e| e.to_string())
}
```

---

## 8. `src-tauri/src/agent_brain.rs` (MODIFY)

```rust
fn build_effective_system_prompt(&self, memory_context: Option<&str>) -> String {
    let mut prompt = self.system_prompt.clone();
    if let Some(mem) = memory_context {
        if !mem.trim().is_empty() {
            prompt.push_str("\n\n<memory_context>\n");
            prompt.push_str(mem);
            prompt.push_str("\n</memory_context>");
        }
    }
    prompt
}
```

Change `decide()`'s signature to accept memory context and use
`build_effective_system_prompt(memory_context)` instead of
`self.system_prompt` directly:
```rust
pub async fn decide(
    &self, leader_response: &str, context: &str, memory_context: Option<&str>,
) -> Result<AgentDecision, AgentError>
```

Apply this to every call path that constructs a request: the primary
brain call, the fallback retry, and the secondary-brain path if
`response_router.rs` has switched to `agent_brain_2`. Do not remove or
alter the existing fallback/secondary-brain logic — only add the memory
context parameter alongside it.

---

## 9. `src-tauri/src/response_router.rs` (MODIFY)

### Session start

At the start of `run_agent_loop`:
```rust
{
    let memory_store = state.memory_store.clone();
    let (session_id, project_brief) = (config.session_id.clone(), config.project_brief.clone());
    let (archived, decayed) = crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        let archived = mem.archive_old_session_facts(&project_brief, &session_id).unwrap_or(0);
        let decayed = mem.decay_stale_importance(&project_brief).unwrap_or(0);
        Ok((archived, decayed))
    }).await.unwrap_or((0, 0));
    if archived > 0 || decayed > 0 {
        eprintln!("[MEMORY] Session start: archived {archived} stale facts, decayed {decayed} entries");
    }
}

let memory_context: String = {
    let memory_store = state.memory_store.clone();
    let (session_id, project_brief, session_type) =
        (config.session_id.clone(), config.project_brief.clone(), config.session_type.clone());
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        match mem.get_project_memory_checked(&project_brief) {
            crate::memory_store::MemoryReadOutcome::Failed(reason) => {
                eprintln!("[MEMORY] context build degraded: {reason}");
            }
            crate::memory_store::MemoryReadOutcome::Empty => {
                eprintln!("[MEMORY] no prior context for this project yet");
            }
            crate::memory_store::MemoryReadOutcome::Found(_) => {}
        }
        Ok(mem.build_memory_context(&session_id, &project_brief, Some(session_type.as_str()), None)
            .unwrap_or_default())
    }).await.unwrap_or_default()
};
let mem_ctx: Option<&str> = if memory_context.is_empty() { None } else { Some(memory_context.as_str()) };

struct PendingAdoptionCheck { model_id: String, topic: String, prompt_excerpt: String }
let mut pending_adoptions: Vec<PendingAdoptionCheck> = Vec::new();
let mut models_consulted_since_last_section: Vec<String> = Vec::new();
let mut iterations_since_last_section: u32 = 0;
let mut blueprint_titles: Vec<String> = Vec::new();
let mut routing_observations: Vec<String> = Vec::new();
```

`get_project_memory_checked` is `&self` (non-mutating) — calling it here
purely to decide which diagnostic line to log costs one extra read but
never double-counts access stats against the real, reinforcing read
`build_memory_context` performs internally.

### Pass memory context to the brain

```rust
let decision = brain.decide(leader_response, &context_str, mem_ctx).await?;
```

### Resolve pending adoption checks (covers both Route and RouteCompare)

At the top of the loop body, right after `decision` is known and before
acting on it:
```rust
if !pending_adoptions.is_empty() {
    let checks = std::mem::take(&mut pending_adoptions);
    let adoptions: Vec<(String, String, bool)> = checks.into_iter().map(|pending| {
        let adopted = match &decision {
            crate::agent_brain::AgentDecision::Blueprint { .. } => true,
            crate::agent_brain::AgentDecision::Route { target_model, prompt, .. }
                if crate::memory_store::detect_topic(prompt) == pending.topic
                    && *target_model != pending.model_id => false,
            _ => {
                let display_name = crate::browser_backend::display_name_for(&pending.model_id);
                leader_response.chars().take(300).collect::<String>()
                    .to_lowercase().contains(&display_name.to_lowercase())
            }
        };
        (pending.model_id, pending.topic, adopted)
    }).collect();

    let memory_store = state.memory_store.clone();
    let (project_brief, session_id) = (config.project_brief.clone(), config.session_id.clone());
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        for (model_id, topic, adopted) in &adoptions {
            mem.record_model_response(&project_brief, model_id, topic, *adopted, &session_id)
                .unwrap_or_else(|e| eprintln!("[MEMORY] adoption record: {e}"));
        }
        Ok(())
    }).await.ok();
}
```

Add `display_name_for()` to `browser_backend.rs` if not already present:
```rust
pub fn display_name_for(agent_id: &str) -> &'static str {
    match agent_id {
        "chatgpt" => "ChatGPT", "claude" => "Claude", "gemini" => "Gemini",
        "deepseek" => "DeepSeek", "qwen" => "Qwen", "glm" => "GLM", "kimi" => "Kimi",
        other => { eprintln!("[MEMORY] display_name_for: unknown agent_id '{other}'"); "Unknown Model" }
    }
}
```

### Route decision

```rust
let routing_topic = crate::memory_store::detect_topic(&prompt).to_string();
{
    let memory_store = state.memory_store.clone();
    let (session_id, project_brief, target, topic, iter) =
        (config.session_id.clone(), config.project_brief.clone(), target_model.clone(), routing_topic.clone(), iteration);
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.add_session_fact(&session_id, &project_brief, "routing",
            &format!("Routed to {} on topic '{}' at iteration {}", target, topic, iter),
            None, "leader", "llm")
    }).await.unwrap_or_else(|e| eprintln!("[MEMORY] route fact: {e}"));
}
pending_adoptions.push(PendingAdoptionCheck {
    model_id: target_model.clone(), topic: routing_topic.clone(),
    prompt_excerpt: crate::memory_store::safe_prefix(&prompt, 160),
});
models_consulted_since_last_section.push(target_model.clone());
routing_observations.push(format!("Consulted {} on {}", target_model, routing_topic));
iterations_since_last_section += 1;
app.emit("memory-updated", serde_json::json!({"memory_type": "session", "trigger": "routing"})).ok();
```

### RouteCompare decision

Same as Route, but push one `PendingAdoptionCheck` per model in the
comparison list:
```rust
let routing_topic = crate::memory_store::detect_topic(&prompt).to_string();
for model in &models {
    pending_adoptions.push(PendingAdoptionCheck {
        model_id: model.clone(), topic: routing_topic.clone(),
        prompt_excerpt: crate::memory_store::safe_prefix(&prompt, 160),
    });
    models_consulted_since_last_section.push(model.clone());
}
iterations_since_last_section += 1;
{
    let memory_store = state.memory_store.clone();
    let (session_id, project_brief, models_joined, topic) =
        (config.session_id.clone(), config.project_brief.clone(), models.join(", "), routing_topic.clone());
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.add_session_fact(&session_id, &project_brief, "routing",
            &format!("Compared {} on topic '{}'", models_joined, topic), None, "leader", "llm")
    }).await.unwrap_or_else(|e| eprintln!("[MEMORY] route_compare fact: {e}"));
}
app.emit("memory-updated", serde_json::json!({"memory_type": "session", "trigger": "route_compare"})).ok();
```

### Blueprint decision

```rust
{
    let memory_store = state.memory_store.clone();
    let (project_brief, title, models, iters) = (
        config.project_brief.clone(), section_title.clone(),
        models_consulted_since_last_section.clone(), iterations_since_last_section,
    );
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.record_blueprint_finalized(&project_brief, &title, &models, iters)
    }).await.unwrap_or_else(|e| eprintln!("[MEMORY] blueprint record: {e}"));
    app.emit("memory-updated", serde_json::json!({"memory_type": "project", "trigger": "blueprint"})).ok();
}
blueprint_titles.push(section_title.clone());
models_consulted_since_last_section.clear();
iterations_since_last_section = 0;
```

### AskUser decision

After `rx.await` receives the answer, if `answer != "Cancelled"`:
```rust
{
    let memory_store = state.memory_store.clone();
    let (project_brief, session_id, content, q_prefix) = (
        config.project_brief.clone(), config.session_id.clone(),
        format!("User answered '{}': {}", crate::memory_store::safe_prefix(&question, 40), answer),
        crate::memory_store::safe_prefix(&question, 30),
    );
    let resolution = format!("User answered: {}", answer);
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        mem.add_project_memory_with_source(&project_brief, "user_preference", &content, None, None, "user", "confirmed")?;
        mem.resolve_question(&session_id, &q_prefix, &resolution).ok();
        Ok(())
    }).await.unwrap_or_else(|e| eprintln!("[MEMORY] user preference: {e}"));
}
app.emit("memory-updated", serde_json::json!({"memory_type": "project", "trigger": "user_answer"})).ok();
```
Uses `add_project_memory_with_source` with `source_agent = "user"`,
`source_type = "confirmed"` — a direct user answer is the one Phase 1
write site with unambiguous, highest-confidence provenance, and must not
default to `"leader"`/`"llm"`.

### Complete decision

```rust
{
    let memory_store = state.memory_store.clone();
    let (session_id, project_brief, iters, sections) = (
        config.session_id.clone(), config.project_brief.clone(), iteration, blueprint_titles.len() as u32,
    );
    let (completed, learned) = (
        blueprint_titles.iter().map(|t| format!("Blueprint section: {}", t)).collect::<Vec<_>>(),
        routing_observations.clone(),
    );
    crate::db_helpers::run_blocking(move || {
        let mut mem = memory_store.lock().unwrap_or_else(|p| p.into_inner());
        let open_qs = mem.get_open_questions(&project_brief).unwrap_or_default();
        let summary = SessionSummaryData {
            investigated: vec![format!("Expert panel consulted on {} over {} iterations", project_brief, iters)],
            completed,
            learned,
            next_steps: open_qs.iter().map(|q| format!("Unresolved: {}", q.question)).collect(),
        };
        mem.write_session_completion_memory(&session_id, &project_brief, &summary, &open_qs, sections, iters)
    }).await.unwrap_or_else(|e| eprintln!("[MEMORY] session summary: {e}"));
    app.emit("memory-updated", serde_json::json!({"memory_type": "project", "trigger": "session_complete"})).ok();
}
```

**All memory operations in this file are non-fatal** — every
`db_helpers::run_blocking` call above is followed by
`.unwrap_or_else(|e| eprintln!(...))` or `.unwrap_or_default()`, never
`?`. A memory failure must never abort the session loop.

---

## 10. IPC.md (MODIFY — add these entries)

### Events
```typescript
listen('memory-updated', (event) => {
    const { memory_type, trigger } = event.payload
    // memory_type: 'session' | 'project' | 'global'
    // trigger: 'routing' | 'route_compare' | 'blueprint' | 'user_answer' | 'session_complete'
})

listen('memory-health-warning', (event) => {
    const { text, fts_needs_repair } = event.payload
})
```

### Commands
```typescript
invoke('get_project_memory', { project_brief: string })       // Returns Promise<string> — JSON.parse()
invoke('get_global_memory')                                    // Returns Promise<string> — JSON.parse()
invoke('clear_project_memory', { project_brief: string })      // Returns Promise<void>
invoke('get_open_questions', { project_brief: string })        // Returns Promise<string> — JSON.parse()
invoke('get_model_strengths', { project_brief: string })       // Returns Promise<string> — JSON.parse()
invoke('save_project_config', { project_brief: string, content: string })  // Returns Promise<void>
invoke('get_project_config', { project_brief: string })        // Returns Promise<string> — plain string, do NOT parse
invoke('get_memory_health')                                     // Returns Promise<string> — JSON.parse()
invoke('repair_memory_index')                                   // Returns Promise<void>
invoke('get_patterns', { project_brief: string })               // Returns Promise<string> — JSON.parse()
invoke('export_memory', { destination_path: string })           // Returns Promise<void>
invoke('restore_memory', { source_path: string })                // Returns Promise<void>
```

---

## 11. Frontend

### `src/panels/SettingsPanel.tsx` (MODIFY)

Add a **Project Context** section after System Prompts: a textarea bound
to `save_project_config`/`get_project_config`, requiring
`currentProjectBrief` to be set before saving, with a note that this
content is hard-pinned and always injected. Render `<MemoryPanel />`
immediately after this section.

### `src/panels/MemoryPanel.tsx` (NEW FILE)

Include:
1. Health status display (from `get_memory_health`)
2. A "Repair Search Index" button, shown only when `fts_needs_repair` is
   true, calling `repair_memory_index`
3. Export Backup button (`@tauri-apps/plugin-dialog`'s `save()` for the
   destination path, then `export_memory`)
4. Restore Backup button (`open()` for the source path, a `confirm()`
   dialog warning that this overwrites current memory, then
   `restore_memory`)
5. "View Stored Facts" button calling `get_project_memory`, rendering a
   scrollable list showing: pin indicator, category, source type,
   importance, content preview
6. "Clear Project Memory" button with a confirmation dialog, calling
   `clear_project_memory`

No inline per-row editing or deletion in Phase 1.

Use whatever existing safe-invoke wrapper this codebase already
standardizes on (check `src/lib/tauri.ts` or equivalent) rather than
calling `invoke` raw, if such a wrapper exists.

### `src/hooks/useIpcListeners.ts` (MODIFY)

Add listeners for `memory-updated` and `memory-health-warning`, cleaned
up on unmount per this file's existing pattern for every other listener.

### `src/stores/useAppStore.ts` (MODIFY, only if needed)

Add state for the current project brief (if not already tracked) and
the most recent `MemoryHealth` (if the health warning listener needs
somewhere to store it for `MemoryPanel.tsx` to read).

---

## 12. File Delivery List

Deliver as complete replacement files, one batch, per the project's
Direct File Delivery Method:

1. `src-tauri/Cargo.toml`
2. `src-tauri/src/memory_store.rs` (new)
3. `src-tauri/src/orchestrator.rs`
4. `src-tauri/src/main.rs`
5. `src-tauri/src/agent_brain.rs`
6. `src-tauri/src/response_router.rs`
7. `src-tauri/src/commands.rs`
8. `src-tauri/src/browser_backend.rs`
9. `IPC.md`
10. `src/panels/SettingsPanel.tsx`
11. `src/panels/MemoryPanel.tsx` (new)
12. `src/hooks/useIpcListeners.ts`

Only if genuinely missing from the current codebase, also:
- `package.json` (dialog plugin)
- `src/stores/useAppStore.ts`

Read every affected file completely before writing any replacement.

---

## 13. Verification

```bash
cd src-tauri && cargo check
```
Must pass with 0 errors.

```bash
grep -R "memory_store.lock().await" src-tauri/src
```
Must return nothing — this store is always accessed via
`db_helpers::run_blocking()` from async contexts, never a direct
`.await` on the lock itself.

```bash
sqlite3 memory.db ".tables"
sqlite3 memory.db "PRAGMA journal_mode;"      # expect: wal
sqlite3 memory.db "PRAGMA user_version;"      # expect: >= 1
```

```bash
cd .. && npm run build
```
Must pass with 0 errors.

**Runtime:**
1. Save Project Context, start a session, confirm the memory context
   passed to the brain includes it.
2. Trigger a `Route`, a `RouteCompare`, a `Blueprint`, and an `AskUser`
   decision in one session; complete the session.
3. Inspect `model_reliability` directly — confirm rows exist for every
   model involved in the `RouteCompare`, not just one.
4. View Stored Facts in the Memory panel; confirm entries appear with
   correct pin/category/source/importance labels.
5. Export memory, then restore it — confirm the automatic pre-restore
   backup file exists in `memory_backups/` before the restore completes.
6. Attempt to restore while a session is active — confirm it's refused.
7. Run the health check; if `fts_needs_repair` is ever true, run the
   repair button and confirm it clears.

---

## 14. After Completion

1. `cargo check` and `npm run build` both pass — git checkpoint.
2. Update DECISIONS.md's Current State section to include the memory
   system as implemented (six tables, provenance tracking, hard-pinning,
   export/restore, health check — matching this document's scope).
3. Per Chat Session Management: this is a significant milestone — start
   a new chat in this Claude Project for Phase 2 (Skills).
