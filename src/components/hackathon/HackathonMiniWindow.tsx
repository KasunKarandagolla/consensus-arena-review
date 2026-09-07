import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { AlertTriangle, ArrowRight, Check, CheckCircle2, ChevronDown, ChevronUp, Cpu, FolderPlus, GripVertical, Info, Loader2, MoreVertical, Pencil, Plus, Send, Settings2, SlidersHorizontal, Trash2, X, XCircle } from 'lucide-react'
import { buildCommandErrorMessage, safeInvoke as invoke } from '@/lib/tauri'
import { useAppStore, type HackathonConfigSafe, type HackathonRunSafe } from '@/stores/useAppStore'

type PopupMode = null | 'model' | 'team'

export default function HackathonMiniWindow() {
  const { hackathonOpen, setHackathonOpen, hackathonConfig, setHackathonConfig, hackathonRun, setHackathonRun, addToast } = useAppStore()
  const [popup, setPopup] = useState<PopupMode>(null)
  const [modelForm, setModelForm] = useState({ model_name: '', base_url: '', api_key: '', group_id: '' })
  const [pendingNames, setPendingNames] = useState<string[]>([])
  const [teamForm, setTeamForm] = useState({ name: '' })
  const [busy, setBusy] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')
  const [selectedParticipants, setSelectedParticipants] = useState<Set<string>>(new Set())
  const [menu, setMenu] = useState<{ modelId: string; groupId: string; x: number; y: number } | null>(null)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [dragging, setDragging] = useState<{ groupId: string; modelId: string; idx: number } | null>(null)
  const [dragOver, setDragOver] = useState<{ groupId: string; idx: number } | null>(null)
  const menuRef = useRef<HTMLDivElement>(null)

  // Load config when window opens
  const loadConfig = useCallback(async () => {
    try {
      const raw = await invoke<string>('get_hackathon_config')
      const cfg = JSON.parse(raw ?? 'null') as HackathonConfigSafe
      if (cfg) setHackathonConfig(cfg)
    } catch (e) {
      console.error('[hackathon] load failed', e)
    }
    try {
      const raw2 = await invoke<string>('get_hackathon_run_state')
      if (raw2 && raw2 !== 'null') {
        const run = JSON.parse(raw2) as HackathonRunSafe
        if (run) setHackathonRun(run)
      }
    } catch {}
  }, [setHackathonConfig, setHackathonRun])

  useEffect(() => {
    if (hackathonOpen) void loadConfig()
  }, [hackathonOpen, loadConfig])

  // Close menu on outside click / Escape
  useEffect(() => {
    if (!menu) return
    const onDown = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) setMenu(null)
    }
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setMenu(null) }
    document.addEventListener('mousedown', onDown)
    document.addEventListener('keydown', onKey)
    return () => { document.removeEventListener('mousedown', onDown); document.removeEventListener('keydown', onKey) }
  }, [menu])

  // Keep selectedParticipants in sync with confirmed responders (default all confirmed selected).
  // §21: After Send Invitation, responded models become selected by default; only confirmed are auto-selected.
  // Preservation of manual deselection for same run is respected, but a new run (new run_id) resets to that run's confirmed set.
  const prevRunIdRef = useRef<string | null>(null)
  useEffect(() => {
    if (!hackathonRun) return
    const confirmedIds = hackathonRun.groups.flatMap(g => g.participants.filter(p => p.status === 'confirmed').map(p => p.model_id))
    if (confirmedIds.length === 0) {
      // No confirmed yet — keep empty, will fill when responders land.
      // If run_id changed and still zero confirmed, clear selection to avoid stale ids from prior run.
      if (prevRunIdRef.current !== hackathonRun.run_id) {
        prevRunIdRef.current = hackathonRun.run_id
        setSelectedParticipants(new Set())
      }
      return
    }
    const isNewRun = prevRunIdRef.current !== hackathonRun.run_id
    if (isNewRun) {
      prevRunIdRef.current = hackathonRun.run_id
      // New run: default to all confirmed for this run (failed models deliberately not selected).
      setSelectedParticipants(new Set(confirmedIds))
      return
    }
    // Same run: auto-add any newly confirmed ids that were not yet selected (for progressive fan-out),
    // but never auto-remove a manually deselected confirmed. This ensures invitations that complete
    // after the user opened the window still become selected, without overriding intentional toggles
    // for already-seen confirmed ids (merge, not overwrite).
    setSelectedParticipants(prev => {
      if (prev.size === 0) return new Set(confirmedIds)
      let shouldUpdate = false
      for (const id of confirmedIds) {
        if (!prev.has(id)) { shouldUpdate = true; break }
      }
      if (!shouldUpdate) return prev
      const next = new Set(prev)
      for (const id of confirmedIds) next.add(id)
      // Ensure no failed/pending id is auto-added — only confirmed.
      return next
    })
  }, [hackathonRun])

  const groups = hackathonConfig?.groups ?? []
  const models = hackathonConfig?.models ?? []
  const countLabel = useMemo(() => `${groups.length} team${groups.length !== 1 ? 's' : ''} · ${models.length} model${models.length !== 1 ? 's' : ''}`, [groups.length, models.length])

  const displayGroups = useMemo(() => {
    if (!hackathonConfig) return []
    return groups.map(g => {
      const runGroup = hackathonRun?.groups.find(rg => rg.group_id === g.id)
      const participants = models.filter(m => m.group_id === g.id)
      // Display order: after invitations, responders float top preserving order (run's sorted order)
      const displayedIds: string[] = runGroup?.model_ids_ordered?.length ? runGroup.model_ids_ordered : g.model_ids
      let status: 'live' | 'pending' | 'dead' = 'pending'
      let countStr = `${participants.length}/${participants.length}`
      let showNote: string | null = null
      let locked = false
      if (runGroup) {
        const confirmed = runGroup.participants.filter(p => p.status === 'confirmed').length
        const total = runGroup.participants.length
        countStr = `${confirmed}/${total}`
        if (runGroup.status === 'locked') {
          status = 'dead'
          showNote = 'Locked — no responders'
          locked = true
        } else if (runGroup.status === 'running') {
          status = 'live'
          showNote = 'Running…'
        } else if (confirmed === total && total > 0) {
          status = 'live'
        } else if (confirmed > 0) {
          status = 'live'
        } else {
          const hasPending = runGroup.participants.some(p => p.status === 'pending')
          if (hasPending) showNote = 'Sorting live as replies land'
          status = 'pending'
        }
      } else {
        status = g.selected ? 'pending' : 'pending'
      }
      return { cfg: g, runGroup, participants, status, countStr, showNote, locked, displayedIds }
    })
  }, [groups, models, hackathonRun, hackathonConfig])

  async function persist(next: HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> }) {
    setSaving(true)
    setError('')
    try {
      await invoke('save_hackathon_config', { config: next })
      const raw = await invoke<string>('get_hackathon_config')
      const cfg = JSON.parse(raw) as HackathonConfigSafe
      setHackathonConfig(cfg)
    } catch (e) {
      const msg = buildCommandErrorMessage('save_hackathon_config', e)
      setError(msg)
      addToast(msg, 5000)
      throw e
    } finally {
      setSaving(false)
    }
  }

  const [apiKeyMap, setApiKeyMap] = useState<Record<string, string>>({})

  async function handleCreateTeam() {
    const name = teamForm.name.trim()
    if (!name) { setError('Team name is required'); return }
    if (groups.some(g => g.name.trim().toLowerCase() === name.toLowerCase())) {
      setError(`Team "${name}" already exists`); return
    }
    const newGroup = { id: `hk-g-${Date.now()}`, name, model_ids: [] as string[], selected: true }
    const nextModels = models.map(m => ({ ...m, api_key: apiKeyMap[m.id] ?? '' }))
    const next: HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> } = {
      groups: [...groups, newGroup],
      models: nextModels,
      max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
      enabled: true,
    }
    try {
      await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
      setTeamForm({ name: '' })
      setPopup(null)
      addToast(`Team ${name} created`)
    } catch {}
  }

  function handleAddPendingName() {
    const name = modelForm.model_name.trim()
    if (!name) { setError('Model name is required'); return }
    if (!/^[a-zA-Z0-9._\-\/]+$/.test(name)) { setError('Model name contains invalid characters'); return }
    const lowerPending = pendingNames.map(n=>n.toLowerCase())
    if (lowerPending.includes(name.toLowerCase())) { setError('Duplicate name in pending batch'); return }
    const existingNames = models.filter(m => editingId ? m.id !== editingId : true).map(m=>m.model_name.toLowerCase())
    if (existingNames.includes(name.toLowerCase())) { setError('Model name already exists in team'); return }
    setPendingNames(prev=>[...prev, name])
    setModelForm(f=>({ ...f, model_name: '' }))
    setError('')
  }

  function openEditModel(modelId: string) {
    const m = models.find(x => x.id === modelId)
    if (!m) return
    setEditingId(modelId)
    setPendingNames([])
    setModelForm({ model_name: m.model_name, base_url: m.base_url, api_key: apiKeyMap[m.id] ?? '', group_id: m.group_id })
    setError('')
    setMenu(null)
    setPopup('model')
  }

  function closeModelPopup() {
    setPopup(null)
    setEditingId(null)
    setPendingNames([])
    setModelForm({ model_name: '', base_url: '', api_key: '', group_id: '' })
    setError('')
  }

  async function handleAddModel() {
    const { base_url, api_key, group_id } = modelForm
    // Edit mode: single model update, preserve position
    if (editingId) {
      const cur = models.find(m => m.id === editingId)
      if (!cur) { setError('Model not found'); return }
      const newName = modelForm.model_name.trim()
      if (!newName) { setError('Model name is required'); return }
      if (!/^[a-zA-Z0-9._\-\/]+$/.test(newName)) { setError('Model name contains invalid characters'); return }
      const duplicate = models.some(m => m.id !== editingId && m.model_name.toLowerCase() === newName.toLowerCase())
      if (duplicate) { setError('Model name already exists in team'); return }
      if (!base_url.trim()) { setError('Base URL is required'); return }
      try { const u = new URL(base_url); if (!['http:', 'https:'].includes(u.protocol) || !u.hostname) throw new Error('bad') } catch { setError('Base URL must be http(s)'); return }
      if (!api_key.trim()) { setError('API key is required'); return }
      if (!group_id) { setError('Select a team'); return }
      // Build next models with updated entry, preserve order
      const nextModels = models.map(m => m.id === editingId ? { ...m, model_name: newName, base_url: base_url.trim(), group_id, api_key: '' } : { ...m, api_key: apiKeyMap[m.id] ?? '' })
      // If group changed, update group model_ids: remove from old group, insert at end of new group (preserve position as last in new group)
      let nextGroups = groups.map(g => ({ ...g }))
      if (cur.group_id !== group_id) {
        nextGroups = nextGroups.map(g => {
          if (g.id === cur.group_id) return { ...g, model_ids: g.model_ids.filter(id => id !== editingId) }
          if (g.id === group_id) return { ...g, model_ids: [...g.model_ids, editingId] }
          return g
        })
      }
      // Update apiKeyMap
      setApiKeyMap(prev => ({ ...prev, [editingId]: api_key }))
      const withKeys = nextModels.map(m => m.id === editingId ? { ...m, api_key } : m)
      const next = {
        groups: nextGroups,
        models: withKeys,
        max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
        enabled: true,
      }
      try {
        await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
        closeModelPopup()
        addToast(`Model ${newName} updated`)
      } catch {}
      return
    }
    // Create mode: bulk creation
    const allNamesRaw = [...pendingNames]
    const cur = modelForm.model_name.trim()
    if (cur) allNamesRaw.push(cur)
    if (allNamesRaw.length===0) { setError('Add at least one model name (use + to queue)'); return }
    if (!base_url.trim()) { setError('Base URL is required'); return }
    try { const u = new URL(base_url); if (!['http:', 'https:'].includes(u.protocol) || !u.hostname) throw new Error('bad') } catch { setError('Base URL must be http(s)'); return }
    if (!api_key.trim()) { setError('API key is required'); return }
    if (!group_id) { setError('Select a team'); return }
    const lowerBatch = new Set<string>()
    for(const n of allNamesRaw){
      const low=n.toLowerCase()
      if(lowerBatch.has(low)){ setError(`Duplicate in batch: ${n}`); return }
      lowerBatch.add(low)
    }
    const existingLower = new Set(models.map(m=>m.model_name.toLowerCase()))
    for(const n of allNamesRaw){
      if(existingLower.has(n.toLowerCase())){ setError(`Model "${n}" already exists`); return }
    }
    const base = base_url.trim()
    const now = Date.now()
    const newEntries: Array<{id:string;model_name:string;base_url:string;group_id:string;api_key:string}> = allNamesRaw.map((nm, idx)=>({
      id: `hk-m-${now}-${idx}`,
      model_name: nm.trim(),
      base_url: base,
      group_id,
      api_key,
    }))
    const nextGroups = groups.map(g => g.id === group_id ? { ...g, model_ids: [...g.model_ids, ...newEntries.map(e=>e.id)] } : g)
    const newMapUpdates: Record<string,string> = {}
    for(const e of newEntries) newMapUpdates[e.id]=e.api_key
    setApiKeyMap(prev=>({ ...prev, ...newMapUpdates }))
    const nextModelsBase = [...models.map(m=>({ ...m, api_key: apiKeyMap[m.id] ?? '' })), ...newEntries]
    const nextModels = nextModelsBase
    const next = {
      groups: nextGroups,
      models: nextModels,
      max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
      enabled: true,
    }
    try {
      await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
      setModelForm({ model_name: '', base_url: '', api_key: '', group_id: '' })
      setPendingNames([])
      setPopup(null)
      addToast(`${newEntries.length} model(s) added to ${groups.find(g=>g.id===group_id)?.name ?? 'team'} — same URL/key/team`)
    } catch {}
  }

  async function handleToggleGroup(groupId: string) {
    if (saving) return
    const dg = displayGroups.find(d => d.cfg.id === groupId)
    if (dg?.locked) return
    const nextGroups = groups.map(g => g.id === groupId ? { ...g, selected: !g.selected } : g)
    const nextModels = models.map(m => ({ ...m, api_key: apiKeyMap[m.id] ?? '' }))
    const next = {
      groups: nextGroups,
      models: nextModels,
      max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
      enabled: hackathonConfig?.enabled ?? true,
    }
    try {
      await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
    } catch {}
  }

  async function handleReorder(groupId: string, modelId: string, direction: 'up' | 'down') {
    const g = groups.find(x => x.id === groupId)
    if (!g) return
    // Use displayed order when a run exists (responders float top), otherwise config order
    const displayed = displayGroups.find(d => d.cfg.id === groupId)?.displayedIds ?? g.model_ids
    const idx = displayed.indexOf(modelId)
    if (idx === -1) return
    const newIdx = direction === 'up' ? idx - 1 : idx + 1
    if (newIdx < 0 || newIdx >= displayed.length) return
    const newOrder = [...displayed]
    const tmp = newOrder[idx]
    newOrder[idx] = newOrder[newIdx]
    newOrder[newIdx] = tmp
    const nextGroups = groups.map(x => x.id === groupId ? { ...x, model_ids: newOrder } : x)
    const nextModels = models.map(m => ({ ...m, api_key: apiKeyMap[m.id] ?? '' }))
    const next = {
      groups: nextGroups,
      models: nextModels,
      max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
      enabled: hackathonConfig?.enabled ?? true,
    }
    try {
      await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
      // If a run exists, patch its transient ordering so displayGroups (which prefers run order) reflects the move immediately.
      if (hackathonRun) {
        const updatedRun = {
          ...hackathonRun,
          groups: hackathonRun.groups.map(gr => gr.group_id === groupId ? { ...gr, model_ids_ordered: newOrder } : gr),
        }
        setHackathonRun(updatedRun as unknown as typeof hackathonRun)
      }
    } catch {}
  }

  async function handleDragReorder(sourceGroupId: string, sourceModelId: string, targetGroupId: string, targetVisualIdx: number) {
    const srcGroup = groups.find(g => g.id === sourceGroupId)
    const tgtGroup = groups.find(g => g.id === targetGroupId)
    if (!srcGroup || !tgtGroup) return
    if (!srcGroup.model_ids.includes(sourceModelId)) return
    // Resolve displayed orders for source/target (responders float top when run exists)
    const srcDisplay = displayGroups.find(d => d.cfg.id === sourceGroupId)?.displayedIds ?? srcGroup.model_ids
    const tgtDisplay = displayGroups.find(d => d.cfg.id === targetGroupId)?.displayedIds ?? tgtGroup.model_ids
    if (sourceGroupId !== targetGroupId) {
      // Cross-team move: remove from source, insert into target at visual position
      // Map visual target idx to actual insertion index in config's model_ids:
      // if targetVisualIdx points before a displayed model, insert before that model's config index; if at end, append.
      const nextGroups = groups.map(g => {
        if (g.id === sourceGroupId) return { ...g, model_ids: g.model_ids.filter(id => id !== sourceModelId) }
        if (g.id === targetGroupId) {
          const cfgArr = [...g.model_ids]
          // Find insertion point in config array对应的visual position
          let insertAt = cfgArr.length
          if (targetVisualIdx < tgtDisplay.length) {
            const visualTargetId = tgtDisplay[targetVisualIdx]
            const cfgIdx = cfgArr.indexOf(visualTargetId)
            if (cfgIdx !== -1) insertAt = cfgIdx
            else insertAt = Math.max(0, Math.min(targetVisualIdx, cfgArr.length))
          }
          // No duplicate
          if (cfgArr.includes(sourceModelId)) return g
          cfgArr.splice(insertAt, 0, sourceModelId)
          return { ...g, model_ids: cfgArr }
        }
        return g
      })
      // No duplicate membership check
      if (tgtGroup.model_ids.includes(sourceModelId)) return
      const nextModels = models.map(m => m.id === sourceModelId ? { ...m, group_id: targetGroupId, api_key: apiKeyMap[m.id] ?? '' } : { ...m, api_key: apiKeyMap[m.id] ?? '' })
      const next = {
        groups: nextGroups,
        models: nextModels,
        max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
        enabled: hackathonConfig?.enabled ?? true,
      }
      try {
        await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
        // Patch transient run so displayGroups (which prefers run order) moves immediately even when a run exists.
        if (hackathonRun) {
          const prevGroups = hackathonRun.groups
          const srcRun = prevGroups.find(gr => gr.group_id === sourceGroupId)
          const tgtRun = prevGroups.find(gr => gr.group_id === targetGroupId)
          if (srcRun && tgtRun) {
            const movingParticipant = srcRun.participants.find(p => p.model_id === sourceModelId)
            const nextOrderSrc = nextGroups.find(gr => gr.id === sourceGroupId)?.model_ids ?? srcRun.model_ids_ordered.filter(id => id !== sourceModelId)
            const nextOrderTgt = nextGroups.find(gr => gr.id === targetGroupId)?.model_ids ?? [...tgtRun.model_ids_ordered, sourceModelId]
            const updatedRunGroups = prevGroups.map(gr => {
              if (gr.group_id === sourceGroupId) {
                return { ...gr, model_ids_ordered: nextOrderSrc, participants: gr.participants.filter(p => p.model_id !== sourceModelId) }
              }
              if (gr.group_id === targetGroupId && movingParticipant) {
                // Insert participant at visual insertion point in run order as well
                const newParticipants = [...gr.participants]
                // Find visual insertion index in run order (same mapping as config)
                let insertAtRun = newParticipants.length
                if (targetVisualIdx < tgtDisplay.length) {
                  const vid = tgtDisplay[targetVisualIdx]
                  const idx = newParticipants.findIndex(p => p.model_id === vid)
                  if (idx !== -1) insertAtRun = idx
                  else insertAtRun = Math.max(0, Math.min(targetVisualIdx, newParticipants.length))
                }
                newParticipants.splice(insertAtRun, 0, { ...movingParticipant, group_id: targetGroupId })
                return { ...gr, model_ids_ordered: nextOrderTgt, participants: newParticipants }
              }
              return gr
            })
            setHackathonRun({ ...hackathonRun, groups: updatedRunGroups } as unknown as typeof hackathonRun)
            // Keep selection state: moving preserves isSelected for that id
          } else {
            // Fallback: just update ordering keys for run groups
            const ordersById = new Map(nextGroups.map(gr => [gr.id, gr.model_ids] as const))
            const patched = prevGroups.map(gr => {
              const ord = ordersById.get(gr.group_id)
              return ord ? { ...gr, model_ids_ordered: ord } : gr
            })
            setHackathonRun({ ...hackathonRun, groups: patched } as unknown as typeof hackathonRun)
          }
        }
      } catch {}
      return
    }
    // Same-team reorder — visual indices
    const srcVisualIdx = srcDisplay.indexOf(sourceModelId)
    if (srcVisualIdx === -1) return
    if (srcVisualIdx === targetVisualIdx) return
    // Build new order in visual space, then persist that visual order as new cfg order
    const visualCopy = [...srcDisplay]
    const [moved] = visualCopy.splice(srcVisualIdx, 1)
    // When moving within same list, target index after removal shifts if target after source
    const adjustedTarget = targetVisualIdx > srcVisualIdx ? targetVisualIdx : targetVisualIdx
    const clamped = Math.max(0, Math.min(adjustedTarget, visualCopy.length))
    visualCopy.splice(clamped, 0, moved)
    const nextGroups = groups.map(x => x.id === sourceGroupId ? { ...x, model_ids: visualCopy } : x)
    const nextModels = models.map(m => ({ ...m, api_key: apiKeyMap[m.id] ?? '' }))
    const next = {
      groups: nextGroups,
      models: nextModels,
      max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
      enabled: hackathonConfig?.enabled ?? true,
    }
    try {
      await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
      if (hackathonRun) {
        const updatedRun = {
          ...hackathonRun,
          groups: hackathonRun.groups.map(gr => gr.group_id === sourceGroupId ? { ...gr, model_ids_ordered: visualCopy, participants: visualCopy.map(id => gr.participants.find(p => p.model_id === id)).filter((p): p is typeof gr.participants[number] => !!p).concat(gr.participants.filter(p => !visualCopy.includes(p.model_id))) } : gr),
        }
        // Reorder participants to match visualCopy while preserving status; any missing participant stays at end.
        setHackathonRun(updatedRun as unknown as typeof hackathonRun)
      }
    } catch {}
  }

  async function handleDeleteModel(modelId: string) {
    const m = models.find(x => x.id === modelId)
    if (!m) return
    if (!confirm(`Delete model "${m.model_name}"?`)) return
    setMenu(null)
    const nextGroups = groups.map(g => ({ ...g, model_ids: g.model_ids.filter(id => id !== modelId) }))
    const nextModels = models.filter(x => x.id !== modelId).map(x => ({ ...x, api_key: apiKeyMap[x.id] ?? '' }))
    const next = {
      groups: nextGroups,
      models: nextModels,
      max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
      enabled: hackathonConfig?.enabled ?? true,
    }
    const nextKeys = { ...apiKeyMap }
    delete nextKeys[modelId]
    setApiKeyMap(nextKeys)
    try {
      await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
      addToast(`Model ${m.model_name} deleted`)
    } catch {}
  }

  async function handleDeleteGroup(groupId: string) {
    const g = groups.find(x => x.id === groupId)
    if (!g) return
    const msg = g.model_ids.length > 0
      ? `Delete team "${g.name}" and its ${g.model_ids.length} model(s)?`
      : `Delete team "${g.name}"?`
    if (!confirm(msg)) return
    const nextGroups = groups.filter(x => x.id !== groupId)
    const idsToRemove = new Set(g.model_ids)
    const nextModels = models.filter(m => !idsToRemove.has(m.id)).map(m => ({ ...m, api_key: apiKeyMap[m.id] ?? '' }))
    const nextKeys = { ...apiKeyMap }
    for (const id of idsToRemove) delete nextKeys[id]
    setApiKeyMap(nextKeys)
    const next = {
      groups: nextGroups,
      models: nextModels,
      max_questions_per_teammate: hackathonConfig?.max_questions_per_teammate ?? 3,
      enabled: hackathonConfig?.enabled ?? true,
    }
    try {
      await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
      addToast(`Team ${g.name} deleted`)
    } catch {}
  }

  async function handleMaxChange(value: string) {
    let parsed: number | null
    if (value === 'Unlimited') parsed = null
    else {
      const trimmed = value.trim()
      if (trimmed === '') { setError('Maximum questions cannot be empty — use Unlimited or a number'); return }
      const num = Number(trimmed)
      if (!Number.isFinite(num) || Number.isNaN(num)) { setError('Maximum questions must be a number'); return }
      if (!Number.isInteger(num)) { setError('Maximum questions must be an integer'); return }
      if (num <= 0) { setError('Maximum questions must be at least 1'); return }
      if (num > 100) { setError('Maximum questions too large (max 100)'); return }
      parsed = num
    }
    setError('')
    const nextModels = models.map(m => ({ ...m, api_key: apiKeyMap[m.id] ?? '' }))
    const next = {
      groups,
      models: nextModels,
      max_questions_per_teammate: parsed as number | null,
      enabled: hackathonConfig?.enabled ?? true,
    }
    try {
      await persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> })
    } catch {}
  }

  function handleToggleParticipant(modelId: string, isConfirmed: boolean){
    if(!isConfirmed) return
    setSelectedParticipants(prev=>{
      const next=new Set(prev)
      if(next.has(modelId)) next.delete(modelId)
      else next.add(modelId)
      return next
    })
  }

  async function handleSendInvitations() {
    setBusy('invite')
    setError('')
    try {
      const raw = await invoke<string>('send_hackathon_invitations')
      const { run_id } = JSON.parse(raw) as { run_id: string }
      addToast('Invitations sent — watching for replies…')
      try {
        const r2 = await invoke<string>('get_hackathon_run_state')
        if (r2 && r2 !== 'null') setHackathonRun(JSON.parse(r2) as HackathonRunSafe)
      } catch {}
      console.debug('[hackathon] invitations run_id', run_id)
    } catch (e) {
      const msg = buildCommandErrorMessage('send_hackathon_invitations', e)
      setError(msg)
      addToast(msg, 6000)
    } finally {
      setBusy(null)
    }
  }

  function handleGo() {
    if (hackathonRun) {
      const selectedGroups = groups.filter(g=>g.selected)
      for(const g of selectedGroups){
        const rg = hackathonRun.groups.find(x=>x.group_id===g.id)
        if(!rg) continue
        if(rg.status==='locked') continue
        const confirmedIds = rg.participants.filter(p=>p.status==='confirmed').map(p=>p.model_id)
        const actuallySelected = confirmedIds.filter(id=>selectedParticipants.has(id))
        if(confirmedIds.length>0 && actuallySelected.length===0){
          setError(`Team "${g.name}" has responders but none selected — deselect team or select at least one responder`)
          addToast(`Select participants for ${g.name} or deselect the team`, 4000)
          return
        }
      }
      const anySelected = hackathonRun.groups.some(g=>{
        const cfg = groups.find(c=>c.id===g.group_id)
        if(!cfg?.selected) return false
        if(g.status==='locked') return false
        return g.participants.some(p=>p.status==='confirmed' && selectedParticipants.has(p.model_id))
      })
      if(!anySelected && hackathonRun.groups.some(g=>g.status!=='locked')){
        setError('No participants selected — select at least one responder before Go')
        return
      }
    }
    if (hackathonConfig) {
      const nextModels = models.map(m => ({ ...m, api_key: apiKeyMap[m.id] ?? '' }))
      const next = {
        groups,
        models: nextModels,
        max_questions_per_teammate: hackathonConfig.max_questions_per_teammate,
        enabled: true,
      }
      void persist(next as unknown as HackathonConfigSafe & { models: Array<{ id: string; model_name: string; base_url: string; group_id: string; api_key: string }> }).catch(()=>{})
    }
    setHackathonOpen(false)
    addToast('Hackathon configuration saved — ready to start session')
  }

  if (!hackathonOpen) return null

  return (
    <div className="ov" style={{ zIndex: 9400 }} onClick={e => { if (e.target === e.currentTarget) setHackathonOpen(false)}}>
      <div className="hk-modal" onClick={e => e.stopPropagation()} role="dialog" aria-modal="true">
        <div className="hk-head">
          <div className="hk-head-icon" style={{ background: 'var(--surface2)', border: '1px solid var(--border)', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Settings2 size={16} />
          </div>
          <div className="hk-head-txt">
            <h2>Configure Hackathon</h2>
            <p>Register models, form teams, confirm who&apos;s live</p>
          </div>
          <button className="hk-close" onClick={() => setHackathonOpen(false)} aria-label="Close"><X size={14} /></button>
        </div>

        <div className="hk-toolbar">
          <button className="hk-tbtn" onClick={() => { setEditingId(null); setPendingNames([]); setModelForm({model_name:'',base_url:'',api_key:'',group_id:''}); setError(''); setPopup('model')}}><Cpu size={12} />Add model</button>
          <button className="hk-tbtn" onClick={() => setPopup('team')}><FolderPlus size={12} />New team</button>
          <div className="hk-toolbar-spacer" />
          <div className="hk-count-chip">{countLabel}</div>
        </div>

        <div className="hk-body">
          {error && <div className="form-error" style={{ marginBottom: 12 }}>{error}</div>}
          {displayGroups.length === 0 ? (
            <div style={{ padding: '24px 0', textAlign: 'center', color: 'var(--t3)', fontSize: 13 }}>
              No teams yet — create a team, then add models.
            </div>
          ) : (
            <div className="hk-columns">
              {displayGroups.map(({ cfg, runGroup, participants, status, countStr, showNote, locked, displayedIds }) => (
                <div
                  key={cfg.id}
                  className={`hk-col${locked ? ' degraded' : ''}`}
                  onDragOver={e => {
                    // Allow dropping onto empty column body
                    if (!dragging) return
                    if (participants.length === 0) {
                      e.preventDefault()
                      if (dragOver?.groupId !== cfg.id || dragOver?.idx !== 0) setDragOver({ groupId: cfg.id, idx: 0 })
                    }
                  }}
                  onDrop={e => {
                    if (!dragging || participants.length !== 0) return
                    e.preventDefault()
                    void handleDragReorder(dragging.groupId, dragging.modelId, cfg.id, 0)
                    setDragging(null)
                    setDragOver(null)
                  }}
                >
                  <div className="hk-col-head">
                    <button
                      className={`hk-check${cfg.selected ? ' on' : ''}${locked ? ' disabled' : ''}`}
                      onClick={() => void handleToggleGroup(cfg.id)}
                      disabled={locked}
                      title={locked ? 'Locked — no responders' : cfg.selected ? 'Selected' : 'Not selected'}
                    >
                      {cfg.selected && !locked && <Check size={10} />}
                    </button>
                    <div className={`hk-col-name${locked ? ' muted' : ''}`}>{cfg.name}</div>
                    <div className={`hk-col-status ${status}`}>
                      <span className={`d${status === 'pending' && runGroup ? ' wait' : ''}`} />
                      {countStr}
                    </div>
                  </div>
                  <div className="hk-col-body">
                    {participants.length === 0 ? (
                      <div
                        style={{ padding: '12px 9px', fontSize: 11, color: 'var(--t3)', textAlign: 'center', border: dragging ? '1px dashed var(--accent-mid)' : '1px dashed transparent', borderRadius: 8, margin: 6 }}
                        onDragOver={e => { if (dragging) { e.preventDefault(); if (dragOver?.groupId !== cfg.id || dragOver?.idx !== 0) setDragOver({ groupId: cfg.id, idx: 0 }) } }}
                        onDrop={e => { if (!dragging) return; e.preventDefault(); void handleDragReorder(dragging.groupId, dragging.modelId, cfg.id, 0); setDragging(null); setDragOver(null) }}
                      >No models in this team{dragging && <span style={{display:'block',fontSize:10,color:'var(--accent)',marginTop:4}}>Drop here to move</span>}</div>
                    ) : (
                      displayedIds.map((mid, idx) => {
                        const m = models.find(x => x.id === mid)
                        if (!m) return null
                        const rp = runGroup?.participants.find(p => p.model_id === mid)
                        const isConfirmed = rp?.status === 'confirmed'
                        const isFailed = rp?.status === 'failed'
                        const isPending = rp?.status === 'pending' || !rp
                        const showSpin = isPending && !!runGroup
                        const host = (() => { try { return new URL(m.base_url).hostname } catch { return m.base_url } })()
                        const isLead = idx === 0
                        const isSelected = selectedParticipants.has(mid)
                        const canSelect = isConfirmed
                        const isDragging = dragging?.modelId === mid && dragging?.groupId === cfg.id
                        const isDragOver = dragOver?.groupId === cfg.id && dragOver?.idx === idx
                        return (
                          <div
                            key={mid}
                            className={`hk-row${isConfirmed ? ' responded' : ''}${isFailed ? ' no-response' : ''}${isDragging ? ' dragging' : ''}${isDragOver ? ' drag-over' : ''}`}
                            draggable={false}
                            onDragOver={e => {
                              if (!dragging) return
                              e.preventDefault()
                              if (dragOver?.groupId !== cfg.id || dragOver?.idx !== idx) setDragOver({ groupId: cfg.id, idx })
                            }}
                            onDrop={e => {
                              e.preventDefault()
                              if (!dragging) return
                              // Insert before the drop target in visual order
                              void handleDragReorder(dragging.groupId, dragging.modelId, cfg.id, idx)
                              setDragging(null)
                              setDragOver(null)
                            }}
                          >
                            {runGroup && (
                              <button
                                className={`hk-check${isSelected && canSelect ? ' on' : ''}${!canSelect ? ' disabled' : ''}`}
                                style={{width:14,height:14,borderRadius:4}}
                                disabled={!canSelect}
                                onClick={() => handleToggleParticipant(mid, canSelect)}
                                title={canSelect ? (isSelected ? 'Selected — will participate' : 'Deselected — will not participate') : 'Cannot select — no response'}
                              >
                                {isSelected && canSelect && <Check size={8} />}
                              </button>
                            )}
                            <div
                              className="hk-drag-handle"
                              draggable
                              onDragStart={e => {
                                e.dataTransfer.effectAllowed = 'move'
                                e.dataTransfer.setData('text/plain', mid)
                                setDragging({ groupId: cfg.id, modelId: mid, idx })
                                setMenu(null)
                              }}
                              onDragEnd={() => { setDragging(null); setDragOver(null) }}
                              title="Drag to reorder"
                              onClick={e => e.stopPropagation()}
                            >
                              <GripVertical size={10} />
                            </div>
                            <div className={`hk-rank${isConfirmed && isLead ? ' pos1' : ''}`}>{idx + 1}</div>
                            <div className="hk-row-info">
                              <div className="hk-row-name">
                                {m.model_name}
                                {isLead && <span className="hk-leader-badge">Lead</span>}
                              </div>
                              <div className="hk-row-meta">{host}</div>
                            </div>
                            <div className={`hk-row-state${isConfirmed ? ' ok' : ''}${isFailed ? ' dead' : ''}`}>
                              {showSpin ? <Loader2 size={12} className="spin-sm" /> : isConfirmed ? <CheckCircle2 size={12} /> : isFailed ? <XCircle size={12} /> : null}
                            </div>
                            <div className="hk-row-hover">
                              <div className="hk-reorder">
                                <button className={`hk-arrow${idx === 0 ? ' disabled' : ''}`} onClick={() => void handleReorder(cfg.id, mid, 'up')} title="Move up"><ChevronUp size={9} /></button>
                                <button className={`hk-arrow${idx === displayedIds.length - 1 ? ' disabled' : ''}`} onClick={() => void handleReorder(cfg.id, mid, 'down')} title="Move down"><ChevronDown size={9} /></button>
                              </div>
                              <button
                                className="hk-more"
                                aria-expanded={menu?.modelId === mid && menu?.groupId === cfg.id}
                                aria-haspopup="menu"
                                onClick={e => {
                                  e.stopPropagation()
                                  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
                                  const x = Math.min(rect.right + 4, window.innerWidth - 140)
                                  const y = Math.min(rect.bottom + 4, window.innerHeight - 120)
                                  // Close if same
                                  if (menu?.modelId === mid && menu?.groupId === cfg.id) setMenu(null)
                                  else setMenu({ modelId: mid, groupId: cfg.id, x, y: rect.top + 22 })
                                  // Position with viewport clamp: ensure menu not under scrollbar
                                  const clampedX = Math.min(x, window.innerWidth - 138)
                                  setMenu({ modelId: mid, groupId: cfg.id, x: clampedX, y: Math.min(y, window.innerHeight - 90) })
                                }}
                                title="More actions"
                              >
                                <MoreVertical size={12} />
                              </button>
                            </div>
                          </div>
                        )
                      })
                    )}
                    {/* Drop zone at end of list for appending — accepts same or cross-team */}
                    {dragging && (
                      <div
                        style={{ height: '12px', margin: '2px 0', borderRadius: 6, background: dragOver?.groupId===cfg.id && dragOver?.idx===displayedIds.length ? 'var(--accent-soft)' : 'transparent', border: dragOver?.groupId===cfg.id && dragOver?.idx===displayedIds.length ? '1px dashed var(--accent-mid)' : '1px dashed transparent' }}
                        onDragOver={e => { e.preventDefault(); if (dragOver?.groupId !== cfg.id || dragOver?.idx !== displayedIds.length) setDragOver({ groupId: cfg.id, idx: displayedIds.length }) }}
                        onDragLeave={() => { if (dragOver?.groupId===cfg.id && dragOver?.idx===displayedIds.length) setDragOver(null) }}
                        onDrop={e => {
                          e.preventDefault()
                          if (!dragging) return
                          void handleDragReorder(dragging.groupId, dragging.modelId, cfg.id, displayedIds.length)
                          setDragging(null)
                          setDragOver(null)
                        }}
                      />
                    )}
                  </div>
                  {showNote && (
                    <div className={`hk-col-note${locked ? ' warn' : ''}`}>
                      {locked ? <AlertTriangle size={10} /> : <Info size={10} />}
                      {showNote}
                    </div>
                  )}
                  {!locked && participants.length > 0 && (
                    <button
                      onClick={() => void handleDeleteGroup(cfg.id)}
                      style={{ fontSize: 11, color: 'var(--t3)', padding: '4px 9px', textAlign: 'left', width: '100%' }}
                    >
                      Delete team
                    </button>
                  )}
                </div>
              ))}
            </div>
          )}

          <div className="hk-rounds">
            <SlidersHorizontal size={13} />
            <div className="hk-rounds-label">Max questions per teammate<span>Leader is exempt from this cap</span></div>
            <label style={{display:'flex',alignItems:'center',gap:8}}>
              <input
                className="fi"
                type="number"
                min={1}
                step={1}
                style={{width:80}}
                placeholder={hackathonConfig?.max_questions_per_teammate===null ? '∞' : ''}
                value={hackathonConfig?.max_questions_per_teammate===null ? '' : String(hackathonConfig?.max_questions_per_teammate ?? 3)}
                onChange={e=>{
                  const v=e.target.value.trim()
                  if(v===''){ void handleMaxChange('Unlimited'); return}
                  void handleMaxChange(v)
                }}
              />
              <span style={{fontSize:11,color:'var(--t3)'}}>or</span>
              <button
                className={`hk-check${hackathonConfig?.max_questions_per_teammate===null ? ' on' : ''}`}
                style={{width:18,height:18}}
                onClick={()=> void handleMaxChange(hackathonConfig?.max_questions_per_teammate===null ? '3' : 'Unlimited')}
                title={hackathonConfig?.max_questions_per_teammate===null ? 'Unlimited — click for 3' : 'Click for Unlimited'}
              >
                <Check size={10}/>
              </button>
              <span style={{fontSize:11,fontWeight:600,color: hackathonConfig?.max_questions_per_teammate===null ? 'var(--accent)' : 'var(--t3)'}}>Unlimited</span>
            </label>
          </div>
        </div>

        <div className="hk-foot">
          <div className="hk-foot-left">
            {displayGroups.some(d => d.locked) ? (
              <><b>{displayGroups.find(d=>d.locked)?.cfg.name}</b> locked — no responders</>
            ) : hackathonRun ? (
              <>{hackathonRun.groups.filter(g=>g.status==='locked').length >0 ? `${hackathonRun.groups.filter(g=>g.status==='locked').length} locked` : `${hackathonRun.groups.length} team(s) ready`}</>
            ) : (
              <>{groups.filter(g=>g.selected).length} team(s) selected</>
            )}
          </div>
          <div className="hk-foot-actions">
            <button className="hk-btn-cancel" onClick={() => setHackathonOpen(false)}>Cancel</button>
            <button className="hk-btn-invite" disabled={busy === 'invite' || groups.filter(g=>g.selected).length===0} onClick={() => void handleSendInvitations()}>
              {busy === 'invite' ? <Loader2 size={13} className="spin-sm" /> : <Send size={13} />}
              {busy === 'invite' ? 'Sending…' : 'Send invitations'}
            </button>
            <button className="hk-btn-go" onClick={handleGo}><ArrowRight size={13} />Go</button>
          </div>
        </div>
      </div>

      {/* Three-dot menu portal - fixed position, not clipped by scroll */}
      {menu && (
        <div ref={menuRef} className="hk-dot-menu" style={{ left: menu.x, top: menu.y }} role="menu">
          <button onClick={() => openEditModel(menu.modelId)} role="menuitem"><Pencil size={12} />Edit</button>
          <button className="danger" onClick={() => void handleDeleteModel(menu.modelId)} role="menuitem"><Trash2 size={12} />Delete</button>
        </div>
      )}

      {popup === 'model' && (
        <div className="hk-popup-layer open" onClick={() => closeModelPopup()}>
          <div className="hk-popup-bg" />
          <div className="hk-popup" onClick={e => e.stopPropagation()}>
            <button className="hk-popup-close" onClick={() => closeModelPopup()}><X size={13} /></button>
            <h3>{editingId ? 'Edit model' : 'Add models'}</h3>
            {!editingId && <p style={{fontSize:11,color:'var(--t3)',marginBottom:8}}>Enter names one by one with <b>+</b>; all share the same URL/key/team. Saved models are individual records.</p>}
            <label>Model name</label>
            <div style={{display:'flex',gap:6}}>
              <input className="fi" style={{flex:1}} placeholder="e.g. deepseek-v3" value={modelForm.model_name} onChange={e => setModelForm({ ...modelForm, model_name: e.target.value })} onKeyDown={e=>{if(e.key==='Enter' && !editingId){e.preventDefault(); handleAddPendingName()}}} />
              {!editingId && <button className="hk-tbtn" style={{padding:'6px 10px',flexShrink:0}} onClick={handleAddPendingName} title="Add another model name (shares URL/key/team)"><Plus size={12}/></button>}
            </div>
            {!editingId && pendingNames.length>0 && (
              <div style={{display:'flex',flexWrap:'wrap',gap:6,marginTop:8,padding:8,background:'var(--surface2)',border:'1px solid var(--border)',borderRadius:8}}>
                {pendingNames.map(n=>(
                  <span key={n} style={{display:'inline-flex',alignItems:'center',gap:4,padding:'3px 8px',background:'var(--surface)',border:'1px solid var(--border2)',borderRadius:20,fontSize:11,fontWeight:600}}>
                    {n}
                    <button onClick={()=>setPendingNames(prev=>prev.filter(x=>x!==n))} style={{display:'flex'}}><X size={10}/></button>
                  </span>
                ))}
                <span style={{fontSize:11,color:'var(--t3)',alignSelf:'center'}}>{pendingNames.length} queued · same URL/key/team</span>
              </div>
            )}
            <label style={{marginTop:10}}>Base URL</label>
            <input className="fi" placeholder="https://integrate.api.nvidia.com/v1" value={modelForm.base_url} onChange={e => setModelForm({ ...modelForm, base_url: e.target.value })} />
            <label>API key</label>
            <input className="fi" type="password" placeholder="nvapi-••••••••" value={modelForm.api_key} onChange={e => setModelForm({ ...modelForm, api_key: e.target.value })} />
            <label>Team</label>
            <select className="fi" value={modelForm.group_id} onChange={e => setModelForm({ ...modelForm, group_id: e.target.value })}>
              <option value="">Select team</option>
              {groups.map(g => <option key={g.id} value={g.id}>{g.name}</option>)}
            </select>
            {groups.length === 0 && <div style={{ fontSize: 11, color: 'var(--amber)', marginTop: 6 }}>Create a team first</div>}
            <div className="hk-popup-actions">
              <button className="hk-btn-g" onClick={() => closeModelPopup()}>Cancel</button>
              <button className="hk-btn-p" disabled={editingId ? (!modelForm.model_name.trim() || !modelForm.base_url.trim() || !modelForm.api_key.trim() || !modelForm.group_id) : ((pendingNames.length===0 && !modelForm.model_name.trim()) || !modelForm.base_url.trim() || !modelForm.api_key.trim() || !modelForm.group_id)} onClick={() => void handleAddModel()}>{editingId ? 'Save changes' : `Save ${pendingNames.length + (modelForm.model_name.trim()?1:0)} model(s)`}</button>
            </div>
          </div>
        </div>
      )}

      {popup === 'team' && (
        <div className="hk-popup-layer open" onClick={() => setPopup(null)}>
          <div className="hk-popup-bg" />
          <div className="hk-popup" onClick={e => e.stopPropagation()}>
            <button className="hk-popup-close" onClick={() => setPopup(null)}><X size={13} /></button>
            <h3>New team</h3>
            <label>Team name</label>
            <input className="fi" placeholder="e.g. Nova" value={teamForm.name} onChange={e => setTeamForm({ ...teamForm, name: e.target.value })} onKeyDown={e=>{if(e.key==='Enter') void handleCreateTeam()}} />
            <div className="hk-popup-actions">
              <button className="hk-btn-g" onClick={() => setPopup(null)}>Cancel</button>
              <button className="hk-btn-p" disabled={!teamForm.name.trim()} onClick={() => void handleCreateTeam()}>Create</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
