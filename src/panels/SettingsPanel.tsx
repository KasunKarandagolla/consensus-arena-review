import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  Activity,
  BookOpen,
  ClipboardCopy,
  Cpu,
  Droplets,
  FileText,
  Info,
  LifeBuoy,
  Moon,
  Palette,
  Route,
  Settings2,
  Sun,
  Wifi,
  X,
} from 'lucide-react'
import {
  buildCommandErrorMessage,
  safeInvoke as invoke,
} from '@/lib/tauri'
import { AGENT_DISPLAY_NAMES, AGENT_IDS } from '@/lib/agents'
import { applyTheme, loadStoredTheme, storeTheme, type Theme } from '@/lib/theme'
import { useAppStore } from '@/stores/useAppStore'
import MemoryPanel from '@/panels/MemoryPanel'

interface Brain { [key: string]: string; api_key: string; base_url: string; model: string; system_prompt: string }
interface Fallback { [key: string]: string; api_key: string; base_url: string; model: string }
interface Health { agent_id: string; is_available: boolean; error_count: number; last_error: string | null }
interface Props { open: boolean; onClose: () => void }
interface LastSettingsError { kind: string; command: string; message: string }
interface DiagnosticSnapshot {
  app_data_dir: string
  settings_db_exists: boolean
  memory_db_exists: boolean
  transcript_db_exists: boolean
  blueprint_db_exists: boolean
  session_active: boolean
  primary_agent_brain_configured: boolean
  fallback_brain_settings_present: boolean
  secondary_brain_configured: boolean
  memory_health: unknown
  leader_window_exists: boolean
  nav_window_exists: boolean
  browser_diagnostics: Array<{
    agent_id: string
    display_name: string
    setup_generation: number
    session_id: string
    selected_leader_id: string
    selected_agent_ids: string[]
    setup_order: string[]
    intended_url: string
    window_label: string
    window_kind: 'leader' | 'nav'
    assigned_window_label: string
    assigned_window_kind: 'leader' | 'nav'
    is_selected_leader: boolean
    created_at: string
    last_navigation_url: string | null
    last_ready_at: string | null
    last_send_detected_at: string | null
    last_response_at: string | null
    last_error: string | null
    current_phase: string
    last_blocker: 'none' | 'captcha_or_challenge' | 'unsupported_url' | 'navigation_error' | 'timeout' | string
    last_blocker_url_redacted: string | null
    last_challenge_detected_at: string | null
    resume_attempt_count: number
    last_resume_at: string | null
    input_found: boolean
    send_button_found: boolean
    last_send_probe_at: string | null
    last_user_submit_event_at: string | null
    last_message_count_seen: number | null
    sent_signal_emitted: boolean
    expected_agent_id: string | null
    last_signal_agent_id: string | null
    last_signal_type: string | null
    last_signal_at: string | null
    stale_signal_count: number
    response_observed_before_send: boolean
    response_observed_after_injection: boolean
    setup_completion_reason: string | null
    prompt_injected_at: string | null
    prompt_injection_error: string | null
    prompt_injection_method: string | null
    prompt_visible_prefix_ok: boolean | null
    prompt_visible_suffix_ok: boolean | null
    prompt_visible_length: number | null
    send_button_enabled_after_injection: boolean | null
    injection_target_tag: string | null
    injection_target_role: string | null
    injection_target_contenteditable: string | null
    readiness_timeout_ms: number | null
    readiness_probe_count: number | null
    input_candidate_count: number | null
    composer_candidate_count: number | null
    send_button_candidate_count: number | null
    page_state_hint: string | null
    page_health_hint: string | null
  }>
  command_timestamp: string
}

const emptyBrain: Brain = { api_key: '', base_url: '', model: '', system_prompt: '' }

function Section({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
  return <section className="sp-sec"><div className="sp-title">{icon}{title}</div>{children}</section>
}

function Save({ busy, label, onClick }: { busy: boolean; label: string; onClick: () => void }) {
  return <button className="sv-btn" disabled={busy} onClick={onClick}>{busy ? 'Saving…' : label}</button>
}

function ErrorDetails({
  error,
  kinds,
  onCopy,
}: {
  error: LastSettingsError | null
  kinds: string[]
  onCopy: (text: string) => void
}) {
  if (!error || !kinds.includes(error.kind)) return null
  return (
    <details style={{ marginTop: 10, border: '1px solid color-mix(in srgb,var(--red) 28%,transparent)', borderRadius: 9, padding: '8px 10px', background: 'color-mix(in srgb,var(--red) 6%,transparent)' }}>
      <summary style={{ color: 'var(--red)', cursor: 'pointer', fontSize: 12.5, fontWeight: 600 }}>
        Last settings error
      </summary>
      <pre style={{ marginTop: 8, whiteSpace: 'pre-wrap', wordBreak: 'break-word', userSelect: 'text', fontSize: 11.5, lineHeight: 1.55, color: 'var(--text)' }}>
        {error.message}
      </pre>
      <button className="sv-btn" style={{ marginTop: 8 }} onClick={() => onCopy(error.message)}>
        <ClipboardCopy size={12} /> Copy error details
      </button>
    </details>
  )
}

export default function SettingsPanel({ open, onClose }: Props) {
  const addToast = useAppStore((state) => state.addToast)
  const setupBrief = useAppStore((state) => state.setupBrief)
  const selectedSessionId = useAppStore((state) => state.selectedSessionId)
  const [brain, setBrain] = useState<Brain>(emptyBrain)
  const [fallback, setFallback] = useState<Fallback>({ api_key: '', base_url: '', model: '' })
  const [secondary, setSecondary] = useState<Brain>(emptyBrain)
  const [leaderPrompt, setLeaderPrompt] = useState('')
  const [participantPrompt, setParticipantPrompt] = useState('')
  const [health, setHealth] = useState<Record<string, Health>>({})
  const [theme, setTheme] = useState<Theme>('blue')
  const [projectBrief, setProjectBrief] = useState('')
  const [projectContext, setProjectContext] = useState('')
  const [busy, setBusy] = useState('')
  const [lastSettingsError, setLastSettingsError] = useState<LastSettingsError | null>(null)
  const [diagnosticSnapshot, setDiagnosticSnapshot] = useState<DiagnosticSnapshot | null>(null)

  const load = useCallback(async () => {
    const results = await Promise.allSettled([
      invoke<string>('get_agent_brain_config'),
      invoke<string>('get_fallback_brain_config'),
      invoke<string>('get_secondary_brain_config'),
      invoke<string>('get_prompt_template', { template_name: 'leader_priming' }),
      invoke<string>('get_prompt_template', { template_name: 'participant_priming' }),
      invoke<string>('get_agent_health'),
    ])
    if (results[0].status === 'fulfilled') try { setBrain(JSON.parse(results[0].value) as Brain) } catch (error) { console.error(error) }
    if (results[1].status === 'fulfilled') try { setFallback(JSON.parse(results[1].value) as Fallback) } catch (error) { console.error(error) }
    if (results[2].status === 'fulfilled') try { setSecondary(JSON.parse(results[2].value) as Brain) } catch (error) { console.error(error) }
    if (results[3].status === 'fulfilled') setLeaderPrompt(results[3].value)
    if (results[4].status === 'fulfilled') setParticipantPrompt(results[4].value)
    if (results[5].status === 'fulfilled') try { setHealth(JSON.parse(results[5].value) as Record<string, Health>) } catch (error) { console.error(error) }
    setTheme(loadStoredTheme())
  }, [])

  useEffect(() => { if (open) void load() }, [load, open])

  useEffect(() => {
    if (!open) return
    let cancelled = false
    async function loadProject() {
      let brief = setupBrief.trim()
      if (!brief && selectedSessionId) {
        try {
          const raw = await invoke<string>('get_session_details', { session_id: selectedSessionId })
          brief = (JSON.parse(raw) as { project_brief: string }).project_brief
        } catch (error) {
          console.error(error)
        }
      }
      if (cancelled) return
      setProjectBrief(brief)
      if (!brief) { setProjectContext(''); return }
      try {
        const content = await invoke<string>('get_project_config', { project_brief: brief })
        if (!cancelled) setProjectContext(content)
      } catch (error) {
        console.error(error)
        if (!cancelled) addToast('Could not load Project Context')
      }
    }
    void loadProject()
    return () => { cancelled = true }
  }, [addToast, open, selectedSessionId, setupBrief])

  useEffect(() => {
    if (!open) return
    const key = (event: KeyboardEvent) => { if (event.key === 'Escape') onClose() }
    document.addEventListener('keydown', key)
    return () => document.removeEventListener('keydown', key)
  }, [onClose, open])

  function reportError(kind: string, command: string, error: unknown) {
    const message = buildCommandErrorMessage(command, error)
    console.error(message)
    setLastSettingsError({ kind, command, message })
    addToast(message, 7000)
  }

  async function save(kind: string, command: string, args: Record<string, string>, message: string) {
    setBusy(kind)
    try {
      await invoke(command, args)
      setLastSettingsError((current) => current?.kind === kind ? null : current)
      addToast(message)
    } catch (error) {
      reportError(kind, command, error)
    } finally {
      setBusy('')
    }
  }

  async function saveProjectContext() {
    if (!projectBrief) { addToast('Select or start a project before saving Project Context'); return }
    await save('project-context', 'save_project_config', { project_brief: projectBrief, content: projectContext }, 'Project Context saved')
  }

  async function showDiagnosticSnapshot() {
    setBusy('diagnostics')
    try {
      const raw = await invoke<string>('get_diagnostic_snapshot')
      setDiagnosticSnapshot(JSON.parse(raw) as DiagnosticSnapshot)
      setLastSettingsError((current) => current?.kind === 'diagnostics' ? null : current)
    } catch (error) {
      reportError('diagnostics', 'get_diagnostic_snapshot', error)
    } finally {
      setBusy('')
    }
  }

  async function copyText(text: string, successMessage: string) {
    try {
      await navigator.clipboard.writeText(text)
      addToast(successMessage)
    } catch {
      addToast('Could not copy; select the text manually')
    }
  }

  function choose(nextTheme: Theme) {
    setTheme(nextTheme)
    storeTheme(nextTheme)
    applyTheme(nextTheme)
  }

  if (!open) return null
  return <>
    <div className="sp-backdrop" onClick={onClose} />
    <aside className="sp">
      <div className="sp-hd"><h3><Settings2 size={17} />Settings</h3><button className="ic-btn" onClick={onClose}><X size={17} /></button></div>
      <div className="sp-body">
        <Section icon={<Wifi size={12} />} title="Connected accounts">
          {AGENT_IDS.map((id) => { const on = health[id]?.is_available; return <div className="cr" key={id}><span className={`cdot ${on ? 'on' : 'off'}`} /><span className="cr-n">{AGENT_DISPLAY_NAMES[id]}</span><button className="cr-btn" disabled title="Account login is managed in each model WebView">{on ? 'Available' : 'Not checked'}</button></div> })}
        </Section>
        <Section icon={<Cpu size={12} />} title="Agent brain">
          <div className="sif"><input className="si2" placeholder="API base URL" value={brain.base_url} onChange={(event) => setBrain({ ...brain, base_url: event.target.value })} /><input className="si2" type="password" placeholder="API key" value={brain.api_key} onChange={(event) => setBrain({ ...brain, api_key: event.target.value })} /><input className="si2" placeholder="Model name" value={brain.model} onChange={(event) => setBrain({ ...brain, model: event.target.value })} /><textarea className="si2" placeholder="System prompt..." value={brain.system_prompt} onChange={(event) => setBrain({ ...brain, system_prompt: event.target.value })} /></div>
          <Save busy={busy === 'primary'} label="Save changes" onClick={() => void save('primary', 'save_agent_brain_config', brain, 'Agent brain saved')} />
          <ErrorDetails error={lastSettingsError} kinds={['primary']} onCopy={(text) => void copyText(text, 'Error details copied')} />
        </Section>
        <Section icon={<LifeBuoy size={12} />} title="Fallback brain">
          <div className="sif"><input className="si2" placeholder="Fallback API base URL" value={fallback.base_url} onChange={(event) => setFallback({ ...fallback, base_url: event.target.value })} /><input className="si2" type="password" placeholder="Fallback API key" value={fallback.api_key} onChange={(event) => setFallback({ ...fallback, api_key: event.target.value })} /><input className="si2" placeholder="Fallback model name" value={fallback.model} onChange={(event) => setFallback({ ...fallback, model: event.target.value })} /></div>
          <Save busy={busy === 'fallback'} label="Save fallback" onClick={() => void save('fallback', 'save_fallback_brain_config', fallback, 'Fallback brain saved')} />
          <ErrorDetails error={lastSettingsError} kinds={['fallback']} onCopy={(text) => void copyText(text, 'Error details copied')} />
        </Section>
        <Section icon={<Route size={12} />} title="Secondary brain">
          <div className="sif"><input className="si2" placeholder="Secondary API base URL" value={secondary.base_url} onChange={(event) => setSecondary({ ...secondary, base_url: event.target.value })} /><input className="si2" type="password" placeholder="Secondary API key" value={secondary.api_key} onChange={(event) => setSecondary({ ...secondary, api_key: event.target.value })} /><input className="si2" placeholder="Secondary model name" value={secondary.model} onChange={(event) => setSecondary({ ...secondary, model: event.target.value })} /><textarea className="si2" placeholder="Secondary system prompt..." value={secondary.system_prompt} onChange={(event) => setSecondary({ ...secondary, system_prompt: event.target.value })} /></div>
          <Save busy={busy === 'secondary'} label="Save secondary" onClick={() => void save('secondary', 'save_secondary_brain_config', secondary, 'Secondary brain saved')} />
          <ErrorDetails error={lastSettingsError} kinds={['secondary']} onCopy={(text) => void copyText(text, 'Error details copied')} />
        </Section>
        <Section icon={<FileText size={12} />} title="System prompts">
          <label className="slbl" style={{ marginTop: 0 }}>Leader priming template</label><textarea className="si2" value={leaderPrompt} onChange={(event) => setLeaderPrompt(event.target.value)} /><Save busy={busy === 'leader'} label="Save" onClick={() => void save('leader', 'save_prompt_template', { template_name: 'leader_priming', content: leaderPrompt }, 'Leader template saved')} />
          <label className="slbl">Participant priming template</label><textarea className="si2" value={participantPrompt} onChange={(event) => setParticipantPrompt(event.target.value)} /><Save busy={busy === 'participant'} label="Save" onClick={() => void save('participant', 'save_prompt_template', { template_name: 'participant_priming', content: participantPrompt }, 'Participant template saved')} />
          <ErrorDetails error={lastSettingsError} kinds={['leader', 'participant']} onCopy={(text) => void copyText(text, 'Error details copied')} />
        </Section>
        <Section icon={<BookOpen size={12} />} title="Project context">
          <textarea className="si2" value={projectContext} disabled={!projectBrief} onChange={(event) => setProjectContext(event.target.value)} placeholder={projectBrief ? 'Record durable constraints, preferences, and decisions for this project.' : 'Select or start a project first.'} />
          <p style={{ fontSize: 11.5, lineHeight: 1.5, color: 'var(--t3)', marginTop: 8 }}>Project Context is hard-pinned and always injected into the agent brain for this project.</p>
          <Save busy={busy === 'project-context'} label="Save Project Context" onClick={() => void saveProjectContext()} />
          <ErrorDetails error={lastSettingsError} kinds={['project-context']} onCopy={(text) => void copyText(text, 'Error details copied')} />
        </Section>
        <MemoryPanel projectBrief={projectBrief} />
        <Section icon={<Activity size={12} />} title="Diagnostics">
          <p style={{ fontSize: 11.5, lineHeight: 1.5, color: 'var(--t3)', marginBottom: 9 }}>Shows database, configuration, and browser-window loading state. Keys, prompts, project content, cookies, and model responses are excluded.</p>
          <button className="sv-btn" disabled={busy === 'diagnostics'} onClick={() => void showDiagnosticSnapshot()}>{busy === 'diagnostics' ? 'Loading…' : 'Show Diagnostic Snapshot'}</button>
          <ErrorDetails error={lastSettingsError} kinds={['diagnostics']} onCopy={(text) => void copyText(text, 'Error details copied')} />
          {diagnosticSnapshot && <details style={{ marginTop: 10, border: '1px solid var(--border)', borderRadius: 9, padding: '8px 10px', background: 'var(--surface2)' }}>
            <summary style={{ cursor: 'pointer', fontSize: 12.5, fontWeight: 600 }}>Diagnostic snapshot</summary>
            <pre style={{ marginTop: 8, whiteSpace: 'pre-wrap', wordBreak: 'break-word', userSelect: 'text', fontSize: 11.5, lineHeight: 1.55, color: 'var(--text)' }}>{JSON.stringify(diagnosticSnapshot, null, 2)}</pre>
            <button className="sv-btn" style={{ marginTop: 8 }} onClick={() => void copyText(JSON.stringify(diagnosticSnapshot, null, 2), 'Diagnostic snapshot copied')}><ClipboardCopy size={12} /> Copy snapshot</button>
          </details>}
        </Section>
        <Section icon={<Palette size={12} />} title="Appearance"><div className="th-g">{([{ t: 'blue', label: 'Blue', icon: <Droplets size={13} /> }, { t: 'light', label: 'Light', icon: <Sun size={13} /> }, { t: 'dark', label: 'Dark', icon: <Moon size={13} /> }] as const).map((item) => <button className={`th-b${theme === item.t ? ' on' : ''}`} onClick={() => choose(item.t)} key={item.t}>{item.icon}{item.label}</button>)}</div></Section>
        <Section icon={<Info size={12} />} title="About"><p style={{ fontSize: 12.5, color: 'var(--t3)' }}>Consensus Arena v0.1.0 · Redesigned 2026</p></Section>
      </div>
    </aside>
  </>
}
