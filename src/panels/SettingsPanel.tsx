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
  Plus,
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
import { refreshParticipants } from '@/lib/agents'
import { useAppStore, type Participant } from '@/stores/useAppStore'
import { applyTheme, loadStoredTheme, storeTheme, type Theme } from '@/lib/theme'
import MemoryPanel from '@/panels/MemoryPanel'

interface Brain { api_key: string; base_url: string; model: string; system_prompt: string }
interface Fallback { api_key: string; base_url: string; model: string }
interface Health { agent_id: string; is_available: boolean; error_count: number; last_error: string | null }
interface CredentialStorageStatus { available: boolean; migration_pending: boolean; message: string }
type SpecialistRole = 'research_lead' | 'product_director' | 'architecture_lead' | 'implementation_engineer' | 'qa_review_lead'
interface SpecialistModel { model_id: string; display_name: string; provider: string; free: boolean; health: { status: string; last_probe_at?: number | null; latency_ms?: number | null } }
interface SpecialistSettings { policy_revision: number; policies: Record<SpecialistRole, { preferred_model: string | null; fallback_models: string[]; custom_model_id?: string | null; enabled: boolean }>; catalog: { models: SpecialistModel[] }; custom_models: { custom_model_id: string; display_name: string; model_name: string; api_key_configured: boolean; test_passed: boolean }[] }
interface ResearchHealth { agent_reach?: { observed_version?: string | null; message: string; version_matches_qualification: boolean }; healthy_channels: number; tiktok: { available: boolean } }
interface Props { open: boolean; onClose: () => void }
interface LastSettingsError { kind: string; command: string; message: string }
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
  const participants = useAppStore((state) => state.participants)
  const setParticipants = useAppStore((state) => state.setParticipants)
  const sessionStatus = useAppStore((state) => state.sessionStatus)
  const isDraftSession = useAppStore((state) => state.isDraftSession)
  const [brain, setBrain] = useState<Brain>(emptyBrain)
  const [fallback, setFallback] = useState<Fallback>({ api_key: '', base_url: '', model: '' })
  const [secondary, setSecondary] = useState<Brain>(emptyBrain)
  const [credentialConfigured, setCredentialConfigured] = useState({ primary: false, fallback: false, secondary: false })
  const [credentialStorageStatus, setCredentialStorageStatus] = useState<CredentialStorageStatus | null>(null)
  const [leaderPrompt, setLeaderPrompt] = useState('')
  const [participantPrompt, setParticipantPrompt] = useState('')
  const [health, setHealth] = useState<Record<string, Health>>({})
  const [theme, setTheme] = useState<Theme>('blue')
  const [projectBrief, setProjectBrief] = useState('')
  const [projectContext, setProjectContext] = useState('')
  const [busy, setBusy] = useState('')
  const [lastSettingsError, setLastSettingsError] = useState<LastSettingsError | null>(null)
  const [diagnosticBrief, setDiagnosticBrief] = useState('')
  const [diagnosticExportPath, setDiagnosticExportPath] = useState('')
  const [maintenanceMode, setMaintenanceMode] = useState(false)
  const [customDraft, setCustomDraft] = useState<Participant | null>(null)
  const [customError, setCustomError] = useState('')
  const [customBusy, setCustomBusy] = useState('')
  const [launchBusy, setLaunchBusy] = useState('')
  const [specialistSettings, setSpecialistSettings] = useState<SpecialistSettings | null>(null)
  const [researchHealth, setResearchHealth] = useState<ResearchHealth | null>(null)
  const [customModelOpen, setCustomModelOpen] = useState(false)
  const [customModelRole, setCustomModelRole] = useState<SpecialistRole | null>(null)
  const [customModel, setCustomModel] = useState({ display_name: '', base_url: '', api_key: '', model_name: '' })

  const load = useCallback(async () => {
    const results = await Promise.allSettled([
      invoke<string>('get_agent_brain_config'),
      invoke<string>('get_fallback_brain_config'),
      invoke<string>('get_secondary_brain_config'),
      invoke<string>('get_prompt_template', { template_name: 'leader_priming' }),
      invoke<string>('get_prompt_template', { template_name: 'participant_priming' }),
      invoke<string>('get_agent_health'),
      invoke<string>('get_participants'),
      invoke<string>('get_maintenance_mode'),
      invoke<string>('get_credential_storage_status'),
      invoke<string>('get_specialist_model_settings'),
      invoke<string>('get_research_capability_health'),
    ])
    if (results[0].status === 'fulfilled') try {
      const value = JSON.parse(results[0].value) as Brain & { api_key_configured?: boolean }
      setBrain(value)
      setCredentialConfigured((current) => ({ ...current, primary: Boolean(value.api_key_configured) }))
    } catch (error) { console.error(error) }
    if (results[1].status === 'fulfilled') try {
      const value = JSON.parse(results[1].value) as Fallback & { api_key_configured?: boolean }
      setFallback(value)
      setCredentialConfigured((current) => ({ ...current, fallback: Boolean(value.api_key_configured) }))
    } catch (error) { console.error(error) }
    if (results[2].status === 'fulfilled') try {
      const value = JSON.parse(results[2].value) as Brain & { api_key_configured?: boolean }
      setSecondary(value)
      setCredentialConfigured((current) => ({ ...current, secondary: Boolean(value.api_key_configured) }))
    } catch (error) { console.error(error) }
    if (results[3].status === 'fulfilled') setLeaderPrompt(results[3].value)
    if (results[4].status === 'fulfilled') setParticipantPrompt(results[4].value)
    if (results[5].status === 'fulfilled') try { setHealth(JSON.parse(results[5].value) as Record<string, Health>) } catch (error) { console.error(error) }
    if (results[6].status === 'fulfilled') try {
      const merged = JSON.parse(results[6].value) as Participant[]
      if (Array.isArray(merged) && merged.length > 0) setParticipants(merged)
    } catch (error) { console.error(error) }
    if (results[7].status === 'fulfilled') try { setMaintenanceMode(JSON.parse(results[7].value) as boolean) } catch { setMaintenanceMode(results[7].value === 'true') }
    if (results[8].status === 'fulfilled') try { setCredentialStorageStatus(JSON.parse(results[8].value) as CredentialStorageStatus) } catch (error) { console.error(error) }
    if (results[9].status === 'fulfilled') try { setSpecialistSettings(JSON.parse(results[9].value) as SpecialistSettings) } catch (error) { console.error(error) }
    if (results[10].status === 'fulfilled') try { setResearchHealth(JSON.parse(results[10].value) as ResearchHealth) } catch (error) { console.error(error) }
    setTheme(loadStoredTheme())
  }, [setParticipants])

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

  async function refreshCredentialStatus() {
    try {
      const raw = await invoke<string>('get_credential_storage_status')
      setCredentialStorageStatus(JSON.parse(raw) as CredentialStorageStatus)
    } catch (error) {
      console.error(error)
    }
  }

  async function save(kind: string, command: string, args: Record<string, string>, message: string) {
    setBusy(kind)
    try {
      await invoke(command, args)
      if (kind === 'primary' || kind === 'fallback' || kind === 'secondary') {
        const clearedFallback = kind === 'fallback'
          && args.api_key === '' && args.base_url === '' && args.model === ''
        const credentialKind = kind as keyof typeof credentialConfigured
        const configuredBeforeSave = credentialConfigured[credentialKind]
        const configuredByThisSave = Boolean(args.api_key?.trim())
        setCredentialConfigured((current) => ({
          ...current,
          [credentialKind]: clearedFallback ? false : configuredByThisSave || configuredBeforeSave,
        }))
        if (kind === 'primary') setBrain((current) => ({ ...current, api_key: '' }))
        if (kind === 'fallback') setFallback((current) => ({ ...current, api_key: '' }))
        if (kind === 'secondary') setSecondary((current) => ({ ...current, api_key: '' }))
        await refreshCredentialStatus()
      }
      setLastSettingsError((current) => current?.kind === kind ? null : current)
      addToast(message)
    } catch (error) {
      reportError(kind, command, error)
    } finally {
      setBusy('')
    }
  }

  async function clearCredential(kind: 'primary' | 'fallback' | 'secondary') {
    setBusy(`clear-${kind}`)
    try {
      await invoke('clear_brain_credential', { kind })
      setCredentialConfigured((current) => ({ ...current, [kind]: false }))
      if (kind === 'primary') setBrain((current) => ({ ...current, api_key: '' }))
      if (kind === 'fallback') setFallback((current) => ({ ...current, api_key: '' }))
      if (kind === 'secondary') setSecondary((current) => ({ ...current, api_key: '' }))
      await refreshCredentialStatus()
      addToast('Saved API key removed from this device')
    } catch (error) {
      reportError(kind, 'clear_brain_credential', error)
    } finally {
      setBusy('')
    }
  }

  async function saveProjectContext() {
    if (!projectBrief) { addToast('Select or start a project before saving Project Context'); return }
    await save('project-context', 'save_project_config', { project_brief: projectBrief, content: projectContext }, 'Project Context saved')
  }

  async function saveSpecialistPolicy(
    role: SpecialistRole,
    preferred: string,
    fallback: string,
    customModelId: string | null = null,
    sourceSettings: SpecialistSettings | null = specialistSettings,
  ) {
    if (!sourceSettings) return
    setBusy(`specialist-${role}`)
    try {
      const raw = await invoke<string>('save_specialist_model_policy', {
        role_family: role,
        preferred_model: customModelId ? null : (preferred || null),
        fallback_models: fallback ? [fallback] : [],
        custom_model_id: customModelId,
        enabled: sourceSettings.policies[role]?.enabled ?? true,
      })
      setSpecialistSettings(JSON.parse(raw) as SpecialistSettings)
      addToast(`${role.replace(/_/g, ' ')} routing saved`)
    } catch (error) {
      reportError('specialist', 'save_specialist_model_policy', error)
    } finally { setBusy('') }
  }

  async function refreshSpecialistCatalog() {
    setBusy('specialist-refresh')
    try {
      const raw = await invoke<string>('refresh_specialist_model_catalog')
      setSpecialistSettings(JSON.parse(raw) as SpecialistSettings)
      addToast('Verified free model catalog refreshed')
    } catch (error) { reportError('specialist', 'refresh_specialist_model_catalog', error) }
    finally { setBusy('') }
  }

  async function saveCustomSpecialistModel() {
    setBusy('specialist-custom')
    try {
      const raw = await invoke<string>('save_custom_specialist_model', customModel)
      const result = JSON.parse(raw) as {
        settings?: SpecialistSettings
        custom_model_id?: string
        last_test?: { passed: boolean; status: string }
        passed?: boolean
        status?: string
      }
      const passed = result.last_test?.passed ?? result.passed
      const status = result.last_test?.status ?? result.status
      if (passed === false) {
        addToast(`Custom model test failed: ${status || 'unavailable'}`, 6000)
        return
      }
      const savedSettings = result.settings
      const customModelId = result.custom_model_id
      const targetRole = customModelRole
      setCustomModel({ display_name: '', base_url: '', api_key: '', model_name: '' })
      setCustomModelOpen(false)
      setCustomModelRole(null)
      if (savedSettings) setSpecialistSettings(savedSettings)
      if (targetRole && savedSettings && customModelId) {
        const fallback = savedSettings.policies[targetRole]?.fallback_models[0] || ''
        await saveSpecialistPolicy(targetRole, '', fallback, customModelId, savedSettings)
      } else {
        await load()
      }
      addToast('Custom model tested and saved securely')
    } catch (error) { reportError('specialist', 'save_custom_specialist_model', error) }
    finally { setBusy('') }
  }

  async function toggleMaintenanceMode() {
    const next = !maintenanceMode
    setBusy('maintenance')
    try {
      await invoke('set_maintenance_mode', { enabled: next })
      setMaintenanceMode(next)
      setLastSettingsError((current) => current?.kind === 'maintenance' ? null : current)
      addToast(next ? 'Maintenance mode enabled' : 'Maintenance mode disabled')
      if (!next) setDiagnosticBrief('')
    } catch (error) {
      reportError('maintenance', 'set_maintenance_mode', error)
    } finally {
      setBusy('')
    }
  }

  async function generateDiagnosticBrief() {
    if (!maintenanceMode) {
      addToast('Enable Maintenance mode to capture diagnostics')
      return
    }
    setBusy('diagnostics')
    try {
      const brief = await invoke<string>('get_diagnostic_brief')
      setDiagnosticBrief(brief)
      setLastSettingsError((current) => current?.kind === 'diagnostics' ? null : current)
    } catch (error) {
      reportError('diagnostics', 'get_diagnostic_brief', error)
    } finally {
      setBusy('')
    }
  }

  async function exportDiagnosticBundle() {
    if (!maintenanceMode) {
      addToast('Enable Maintenance mode to export diagnostic evidence')
      return
    }
    setBusy('diagnostic-export')
    try {
      const raw = await invoke<string>('export_browser_diagnostics')
      const result = JSON.parse(raw) as { export_dir: string }
      setDiagnosticExportPath(result.export_dir)
      setLastSettingsError((current) => current?.kind === 'diagnostic-export' ? null : current)
      addToast('Diagnostic bundle saved on this device')
    } catch (error) {
      reportError('diagnostic-export', 'export_browser_diagnostics', error)
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

  // ── Custom AI CRUD ──────────────────────────────────────────────────────────
  // Custom participants are stored backend-side as a JSON array. The save
  // command validates ids/URLs and rejects built-in collisions, so we keep the
  // shared client-side checks light (non-empty display name + absolute http(s)
  // URL) and let the backend give authoritative validation errors.

  function beginAddCustom() {
    setCustomError('')
    setCustomDraft({ agent_id: '', display_name: '', base_url: '', is_custom: true })
  }

  function beginEditCustom(p: Participant) {
    setCustomError('')
    setCustomDraft({ ...p })
  }

  async function saveCustom(draft: Participant): Promise<boolean> {
    const display = draft.display_name.trim()
    const url = draft.base_url.trim()
    if (!display) { setCustomError('Display name is required.'); return false }
    if (!/^https?:\/\/.+/i.test(url)) { setCustomError('Chat URL must be an absolute http:// or https:// URL.'); return false }

    // Derive a unique id from display name + same-host suffix to avoid
    // collisions; explicit agent_id is not exposed to the user.
    let id = draft.agent_id.trim()
    if (!id) {
      const key = display.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'custom'
      id = key
      let n = 1
      const existing = participants.map((p) => p.agent_id)
      while (existing.includes(id)) { id = `${key}-${n++}` }
    }

    const current: Participant[] = participants.filter((p) => p.is_custom)
    const updated = current.map((p) => (p.agent_id === draft.agent_id && draft.agent_id ? { ...p, display_name: display, base_url: url } : p))
    if (!updated.some((p) => p.agent_id === id)) {
      updated.push({ agent_id: id, display_name: display, base_url: url, is_custom: true })
    }

    setCustomBusy('save')
    try {
      await invoke('save_custom_participants', { participants: updated })
      setCustomError('')
      setCustomDraft(null)
      setParticipants(await refreshParticipants())
      addToast('Custom AI saved')
      return true
    } catch (error) {
      const message = buildCommandErrorMessage('save_custom_participants', error)
      console.error(message)
      setCustomError(message)
      return false
    } finally {
      setCustomBusy('')
    }
  }

  async function deleteCustom(p: Participant) {
    if (!confirm(`Remove "${p.display_name}" from the panel? This only removes the custom participant; session data is untouched.`)) return
    setCustomBusy(p.agent_id)
    try {
      const updated = participants.filter((x) => x.is_custom && x.agent_id !== p.agent_id)
      await invoke('save_custom_participants', { participants: updated })
      if (customDraft?.agent_id === p.agent_id) setCustomDraft(null)
      setParticipants(await refreshParticipants())
      addToast('Custom AI removed')
    } catch (error) {
      reportError('custom', 'save_custom_participants', error)
    } finally {
      setCustomBusy('')
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
        {credentialStorageStatus && (!credentialStorageStatus.available || credentialStorageStatus.migration_pending) && <div className="form-error" role="status" style={{ marginBottom: 12 }}>{credentialStorageStatus.message}</div>}
        <Section icon={<Wifi size={12} />} title="Connected accounts">
          {participants.map((p) => {
            const on = health[p.agent_id]?.is_available
            // §10-12: Draft Setup (New Session before Start) is NOT an active session — Settings/Connected Accounts remain accessible.
            // Only the real backend-active lifecycle (post-start_session) counts as active. `isDraftSession` distinguishes the two.
            const isActiveSession = !isDraftSession && (sessionStatus === 'running' || sessionStatus === 'priming' || sessionStatus === 'setup' || sessionStatus === 'requirements' || sessionStatus === 'paused')
            const anyLaunchBusy = launchBusy !== ''
            async function launch() {
              if (isActiveSession) { addToast('Cannot launch while a session is active. Stop the session first.', 5000); return }
              if (anyLaunchBusy) { addToast('A model window launch is already in progress — wait about 30s.', 3000); return }
              setLaunchBusy(p.agent_id)
              try {
                await invoke('launch_connected_account', { agent_id: p.agent_id })
                addToast(`Navigation started for ${p.display_name} — window loading ${p.base_url}. Complete any login there if prompted.`)
              } catch (error) {
                reportError('launch', 'launch_connected_account', error)
              } finally { setLaunchBusy('') }
            }
            return <div className="cr" key={p.agent_id}><span className={`cdot ${on ? 'on' : 'off'}`} /><span className="cr-n">{p.display_name}</span><span className="cr-btns" style={{ display: 'inline-flex', gap: 6, marginLeft: 'auto' }}><button className="cr-btn" onClick={() => void launch()} disabled={anyLaunchBusy || isActiveSession} title={isActiveSession ? 'Stop the active session before launching a model window (reuses the shared WebView)' : anyLaunchBusy ? 'Another model window is launching — wait a moment' : `Open ${p.display_name} in the app window for login/inspection (navigation will start, not imply ready)`}>{launchBusy === p.agent_id ? 'Opening…' : anyLaunchBusy ? 'Busy…' : 'Launch'}</button><button className="cr-btn" disabled title={p.is_custom ? 'Custom AI — log in manually in its window' : 'Account login is managed in each model WebView'}>{on ? 'Available' : 'Not checked'}</button></span></div>
          })}
          <p style={{ fontSize: 11.5, lineHeight: 1.5, color: 'var(--t3)', marginTop: 8 }}>Google sign-in can be unavailable in embedded model windows, including Gemini. Prefer provider-native login methods where offered.</p>
        </Section>
        <Section icon={<Plus size={14} />} title="Custom AI">
          {participants.filter((p) => p.is_custom).map((p) => (
            <div className="cr" key={p.agent_id}>
              <span className="cdot custom" />
              <span className="cr-n">{p.display_name}</span>
              <span className="cr-u" title={p.base_url}>{p.agent_id}</span>
              <button className="cr-btn" onClick={() => beginEditCustom(p)}>Edit</button>
              <button className="cr-btn cr-del" onClick={() => void deleteCustom(p)} disabled={customBusy === p.agent_id}>Delete</button>
            </div>
          ))}
          {participants.filter((p) => p.is_custom).length === 0 && (
            <div className="sif" style={{ color: 'var(--t2)', fontSize: 12.5 }}>No custom AI added yet. Add a chat service by its URL.</div>
          )}
          {customDraft ? (
            <div className="sif">
              <input className="si2" placeholder="Display name" value={customDraft.display_name} onChange={(e) => setCustomDraft({ ...customDraft, display_name: e.target.value })} />
              <input className="si2" placeholder="Chat URL (https://…)" value={customDraft.base_url} onChange={(e) => setCustomDraft({ ...customDraft, base_url: e.target.value })} />
              {customError && <div className="form-error">{customError}</div>}
              <div className="sact">
                <button className="sv-btn" disabled={customBusy === 'save'} onClick={() => void saveCustom(customDraft)}>{customBusy === 'save' ? 'Saving…' : 'Save'}</button>
                <button className="sv-btn" onClick={() => { setCustomDraft(null); setCustomError('') }}>Cancel</button>
              </div>
            </div>
          ) : (
            <button className="sv-btn" onClick={beginAddCustom}><Plus size={13} /> Add custom AI</button>
          )}
        </Section>
        <Section icon={<Cpu size={12} />} title="Agent brain">
          <div className="sif"><input className="si2" placeholder="API base URL" value={brain.base_url} onChange={(event) => setBrain({ ...brain, base_url: event.target.value })} /><input className="si2" type="password" placeholder={credentialConfigured.primary ? 'Saved securely; leave blank to keep this key' : 'API key'} value={brain.api_key} onChange={(event) => setBrain({ ...brain, api_key: event.target.value })} /><input className="si2" placeholder="Model name" value={brain.model} onChange={(event) => setBrain({ ...brain, model: event.target.value })} /><textarea className="si2" placeholder="System prompt..." value={brain.system_prompt} onChange={(event) => setBrain({ ...brain, system_prompt: event.target.value })} /></div>
          <Save busy={busy === 'primary'} label="Save changes" onClick={() => void save('primary', 'save_agent_brain_config', { api_key: brain.api_key, base_url: brain.base_url, model: brain.model, system_prompt: brain.system_prompt }, 'Agent brain saved')} />
          {credentialConfigured.primary && <button className="sv-btn" disabled={busy !== ''} onClick={() => void clearCredential('primary')}>Remove saved API key</button>}
          <ErrorDetails error={lastSettingsError} kinds={['primary']} onCopy={(text) => void copyText(text, 'Error details copied')} />
        </Section>
        <Section icon={<Route size={12} />} title="Product OS specialist models">
          <p style={{ fontSize: 11.5, lineHeight: 1.5, color: 'var(--t3)', marginTop: 0 }}>
            Configure first-level roles only. Descendant specialists inherit the root role snapshot.
          </p>
          <button className="sv-btn" disabled={busy !== ''} onClick={() => void refreshSpecialistCatalog()}>
            {busy === 'specialist-refresh' ? 'Testing catalog…' : 'Refresh and test free models'}
          </button>
          {specialistSettings && (['research_lead', 'product_director', 'architecture_lead', 'implementation_engineer', 'qa_review_lead'] as SpecialistRole[]).map((role) => {
            const policy = specialistSettings.policies[role] || { preferred_model: null, fallback_models: [], custom_model_id: null, enabled: true }
            const verified = specialistSettings.catalog.models.filter(model => model.free && model.health.status === 'healthy')
            const healthyCustom = specialistSettings.custom_models.filter(custom =>
              custom.test_passed
              && custom.api_key_configured
              && specialistSettings.catalog.models.some(model => model.model_id === custom.custom_model_id && model.health.status === 'healthy')
            )
            const selectedModel = policy.custom_model_id || policy.preferred_model || ''
            const health = specialistSettings.catalog.models.find(model => model.model_id === selectedModel)?.health.status || 'unknown'
            return <div key={role} style={{ borderTop: '1px solid var(--border)', paddingTop: 10, marginTop: 10 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6 }}>
                <span style={{ fontSize: 12.5, fontWeight: 600, textTransform: 'capitalize' }}>{role.replace(/_/g, ' ')}</span>
                <span style={{ fontSize: 11, color: health === 'healthy' ? 'var(--green)' : 'var(--t3)' }}>{health}</span>
              </div>
              <select className="si2" value={selectedModel} onChange={(event) => {
                const value = event.target.value
                if (value === '__configure_custom__') {
                  setCustomModelRole(role)
                  setCustomModelOpen(true)
                  return
                }
                const custom = healthyCustom.find(model => model.custom_model_id === value)
                void saveSpecialistPolicy(role, custom ? '' : value, policy.fallback_models[0] || '', custom?.custom_model_id || null)
              }}>
                <option value="">No verified model selected</option>
                {verified.map(model => <option value={model.model_id} key={model.model_id}>{model.display_name || model.model_id}</option>)}
                {healthyCustom.map(model => <option value={model.custom_model_id} key={model.custom_model_id}>{model.display_name} · custom API ✓</option>)}
                <option value="__configure_custom__">Configure custom API model…</option>
              </select>
              <select className="si2" value={policy.fallback_models[0] || ''} onChange={(event) => void saveSpecialistPolicy(role, policy.preferred_model || '', event.target.value, policy.custom_model_id || null)} style={{ marginTop: 6 }}>
                <option value="">No fallback</option>
                {verified.filter(model => model.model_id !== policy.preferred_model).map(model => <option value={model.model_id} key={model.model_id}>{model.display_name || model.model_id}</option>)}
              </select>
            </div>
          })}
          {!specialistSettings?.catalog.models.some(model => model.free && model.health.status === 'healthy') && <div style={{ color: 'var(--t3)', fontSize: 11.5, marginTop: 8 }}>No model is selectable until a bounded response probe passes.</div>}
          {customModelOpen && <div className="sif" style={{ marginTop: 10 }}>
            <input className="si2" placeholder="Display name" value={customModel.display_name} onChange={(event) => setCustomModel({ ...customModel, display_name: event.target.value })} />
            <input className="si2" placeholder="Base URL" value={customModel.base_url} onChange={(event) => setCustomModel({ ...customModel, base_url: event.target.value })} />
            <input className="si2" placeholder="Model name / ID" value={customModel.model_name} onChange={(event) => setCustomModel({ ...customModel, model_name: event.target.value })} />
            <input className="si2" type="password" placeholder="API key (stored in OS credential storage)" value={customModel.api_key} onChange={(event) => setCustomModel({ ...customModel, api_key: event.target.value })} />
            <div className="sact"><button className="sv-btn" disabled={busy === 'specialist-custom'} onClick={() => void saveCustomSpecialistModel()}>{busy === 'specialist-custom' ? 'Testing…' : 'Test and save'}</button><button className="sv-btn" onClick={() => { setCustomModelOpen(false); setCustomModelRole(null) }}>Cancel</button></div>
          </div>}
        </Section>
        <Section icon={<Wifi size={12} />} title="Research capabilities">
          <div style={{ fontSize: 12, color: 'var(--t2)' }}>Agent Reach: {researchHealth?.agent_reach?.observed_version || 'unavailable'} · healthy channels: {researchHealth?.healthy_channels ?? 0}</div>
          <div style={{ fontSize: 12, color: researchHealth?.tiktok.available ? 'var(--green)' : 'var(--t3)', marginTop: 5 }}>TikTok tt: {researchHealth?.tiktok.available ? 'available' : 'unavailable'}</div>
          <p style={{ fontSize: 11.5, lineHeight: 1.5, color: 'var(--t3)', marginTop: 8 }}>Unavailable mandatory channels remain inconclusive; Arena never treats them as empty evidence.</p>
        </Section>
        <Section icon={<LifeBuoy size={12} />} title="Fallback brain">
          <div className="sif"><input className="si2" placeholder="Fallback API base URL" value={fallback.base_url} onChange={(event) => setFallback({ ...fallback, base_url: event.target.value })} /><input className="si2" type="password" placeholder={credentialConfigured.fallback ? 'Saved securely; leave blank to keep this key' : 'Fallback API key'} value={fallback.api_key} onChange={(event) => setFallback({ ...fallback, api_key: event.target.value })} /><input className="si2" placeholder="Fallback model name" value={fallback.model} onChange={(event) => setFallback({ ...fallback, model: event.target.value })} /></div>
          <Save busy={busy === 'fallback'} label="Save fallback" onClick={() => void save('fallback', 'save_fallback_brain_config', { api_key: fallback.api_key, base_url: fallback.base_url, model: fallback.model }, 'Fallback brain saved')} />
          {credentialConfigured.fallback && <button className="sv-btn" disabled={busy !== ''} onClick={() => void clearCredential('fallback')}>Remove saved API key</button>}
          <ErrorDetails error={lastSettingsError} kinds={['fallback']} onCopy={(text) => void copyText(text, 'Error details copied')} />
        </Section>
        <Section icon={<Route size={12} />} title="Secondary brain">
          <div className="sif"><input className="si2" placeholder="Secondary API base URL" value={secondary.base_url} onChange={(event) => setSecondary({ ...secondary, base_url: event.target.value })} /><input className="si2" type="password" placeholder={credentialConfigured.secondary ? 'Saved securely; leave blank to keep this key' : 'Secondary API key'} value={secondary.api_key} onChange={(event) => setSecondary({ ...secondary, api_key: event.target.value })} /><input className="si2" placeholder="Secondary model name" value={secondary.model} onChange={(event) => setSecondary({ ...secondary, model: event.target.value })} /><textarea className="si2" placeholder="Secondary system prompt..." value={secondary.system_prompt} onChange={(event) => setSecondary({ ...secondary, system_prompt: event.target.value })} /></div>
          <Save busy={busy === 'secondary'} label="Save secondary" onClick={() => void save('secondary', 'save_secondary_brain_config', { api_key: secondary.api_key, base_url: secondary.base_url, model: secondary.model, system_prompt: secondary.system_prompt }, 'Secondary brain saved')} />
          {credentialConfigured.secondary && <button className="sv-btn" disabled={busy !== ''} onClick={() => void clearCredential('secondary')}>Remove saved API key</button>}
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
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 12 }}>
            <span style={{ fontSize: 12.5, fontWeight: 600, color: 'var(--text)' }}>Maintenance mode</span>
            <button
              role="switch"
              aria-checked={maintenanceMode}
              aria-label="Maintenance mode"
              className={`tgl ${maintenanceMode ? 'on' : ''}`}
              style={{ flexShrink: 0 }}
              disabled={busy === 'maintenance'}
              onClick={() => void toggleMaintenanceMode()}
            />
            <span style={{ fontSize: 11.5, fontWeight: 500, color: maintenanceMode ? 'var(--accent)' : 'var(--t3)' }}>{maintenanceMode ? 'ON' : 'OFF'}</span>
          </div>
          <button
            className="sv-btn"
            style={{ marginTop: 0 }}
            disabled={busy === 'diagnostics' || !maintenanceMode}
            title={maintenanceMode ? 'Generate compact diagnostic brief' : 'Enable Maintenance mode to capture diagnostics'}
            onClick={() => void generateDiagnosticBrief()}
          >
            {busy === 'diagnostics' ? 'Generating…' : 'Generate Diagnostic Brief'}
          </button>
          <p style={{ fontSize: 11.5, lineHeight: 1.5, color: 'var(--t3)', marginTop: 8 }}>App logs stay on this device for 14 days, with up to 15 daily files; the Diagnostic Brief shows their folder. Diagnostic bundles contain browser activity evidence, not app logs or Delivery receipts, and Arena checks them against saved API keys. Up to five recent exports are kept; older matching folders are pruned when a new export starts. Nothing is uploaded automatically.</p>
          <button className="sv-btn" style={{ marginTop: 8 }} disabled={busy === 'diagnostic-export' || !maintenanceMode} onClick={() => void exportDiagnosticBundle()}>{busy === 'diagnostic-export' ? 'Exporting…' : 'Export Diagnostic Bundle'}</button>
          {diagnosticExportPath && <div style={{ marginTop: 8, fontSize: 11.5, color: 'var(--t2)', overflowWrap: 'anywhere' }}><div>Saved to: {diagnosticExportPath}</div><button className="sv-btn" style={{ marginTop: 6 }} onClick={() => void copyText(diagnosticExportPath, 'Diagnostic export location copied')}><ClipboardCopy size={12} /> Copy export location</button></div>}
          <ErrorDetails error={lastSettingsError} kinds={['diagnostics', 'diagnostic-export', 'maintenance']} onCopy={(text) => void copyText(text, 'Error details copied')} />
          {diagnosticBrief && <details open style={{ marginTop: 10, border: '1px solid var(--border)', borderRadius: 9, padding: '8px 10px', background: 'var(--surface2)' }}>
            <summary style={{ cursor: 'pointer', fontSize: 12.5, fontWeight: 600 }}>Diagnostic Brief ({Array.from(diagnosticBrief).length.toLocaleString()} characters)</summary>
            <pre style={{ marginTop: 8, maxHeight: 360, overflow: 'auto', whiteSpace: 'pre-wrap', wordBreak: 'break-word', userSelect: 'text', fontSize: 11.5, lineHeight: 1.55, color: 'var(--text)' }}>{diagnosticBrief}</pre>
            <button className="sv-btn" style={{ marginTop: 8 }} onClick={() => void copyText(diagnosticBrief, 'Diagnostic Brief copied')}><ClipboardCopy size={12} /> Copy Diagnostic Brief</button>
          </details>}
        </Section>
        <Section icon={<Palette size={12} />} title="Appearance"><div className="th-g">{([{ t: 'blue', label: 'Blue', icon: <Droplets size={13} /> }, { t: 'light', label: 'Light', icon: <Sun size={13} /> }, { t: 'dark', label: 'Dark', icon: <Moon size={13} /> }] as const).map((item) => <button className={`th-b${theme === item.t ? ' on' : ''}`} onClick={() => choose(item.t)} key={item.t}>{item.icon}{item.label}</button>)}</div></Section>
        <Section icon={<Info size={12} />} title="About"><p style={{ fontSize: 12.5, color: 'var(--t3)' }}>Consensus Arena v0.1.0 · Redesigned 2026</p></Section>
      </div>
    </aside>
  </>
}
