import { useCallback, useEffect, useState } from 'react'
import { Database, Download, Eye, Trash2, Upload, Wrench } from 'lucide-react'
import { confirm as confirmDialog, open, save } from '@tauri-apps/plugin-dialog'
import { safeInvoke as invoke } from '@/lib/tauri'
import { useAppStore } from '@/stores/useAppStore'

interface MemoryHealth {
  is_healthy: boolean
  issues: string[]
  warnings: string[]
  table_counts: Record<string, number>
  fts_needs_repair: boolean
}

interface ProjectMemoryEntry {
  id: string
  category: string
  content: string
  importance: string
  hard_pinned: boolean
  source_type: string
}

interface Props {
  projectBrief: string
}

function preview(content: string): string {
  const characters = Array.from(content)
  return characters.length > 180 ? `${characters.slice(0, 180).join('')}…` : content
}

export default function MemoryPanel({ projectBrief }: Props) {
  const addToast = useAppStore((state) => state.addToast)
  const [health, setHealth] = useState<MemoryHealth | null>(null)
  const [facts, setFacts] = useState<ProjectMemoryEntry[] | null>(null)
  const [busy, setBusy] = useState('')

  const loadHealth = useCallback(async () => {
    try {
      const raw = await invoke<string>('get_memory_health')
      setHealth(JSON.parse(raw) as MemoryHealth)
    } catch (error) {
      console.error(error)
      addToast('Could not read memory health')
    }
  }, [addToast])

  const loadFacts = useCallback(async () => {
    if (!projectBrief) {
      addToast('Select or start a project first')
      return
    }
    setBusy('facts')
    try {
      const raw = await invoke<string>('get_project_memory', {
        project_brief: projectBrief,
      })
      setFacts(JSON.parse(raw) as ProjectMemoryEntry[])
    } catch (error) {
      console.error(error)
      addToast('Could not load stored facts')
    } finally {
      setBusy('')
    }
  }, [addToast, projectBrief])

  useEffect(() => {
    void loadHealth()
  }, [loadHealth])

  async function repairIndex() {
    setBusy('repair')
    try {
      await invoke('repair_memory_index')
      await loadHealth()
      addToast('Memory search index repaired')
    } catch (error) {
      console.error(error)
      addToast('Search index repair failed')
    } finally {
      setBusy('')
    }
  }

  async function exportBackup() {
    try {
      const path = await save({
        defaultPath: 'consensus-arena-memory.db',
        filters: [{ name: 'SQLite database', extensions: ['db'] }],
      })
      if (!path) return
      setBusy('export')
      await invoke('export_memory', { destination_path: path })
      addToast('Memory backup exported')
    } catch (error) {
      console.error(error)
      addToast('Memory export failed')
    } finally {
      setBusy('')
    }
  }

  async function restoreBackup() {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: 'SQLite database', extensions: ['db'] }],
      })
      const path = Array.isArray(selected) ? selected[0] : selected
      if (!path) return
      const confirmed = await confirmDialog(
        'Restore this backup? Current memory will be overwritten after an automatic pre-restore backup is created.',
        { title: 'Restore memory backup', kind: 'warning' },
      )
      if (!confirmed) return
      setBusy('restore')
      await invoke('restore_memory', { source_path: path })
      await loadHealth()
      if (projectBrief) await loadFacts()
      addToast('Memory backup restored')
    } catch (error) {
      console.error(error)
      addToast(String(error))
    } finally {
      setBusy('')
    }
  }

  async function clearMemory() {
    if (!projectBrief) {
      addToast('Select or start a project first')
      return
    }
    const confirmed = await confirmDialog(
      'Clear all session facts, project memory, questions, reliability records, and patterns for this project?',
      { title: 'Clear project memory', kind: 'warning' },
    )
    if (!confirmed) return
    setBusy('clear')
    try {
      await invoke('clear_project_memory', { project_brief: projectBrief })
      setFacts([])
      await loadHealth()
      addToast('Project memory cleared')
    } catch (error) {
      console.error(error)
      addToast('Could not clear project memory')
    } finally {
      setBusy('')
    }
  }

  const statusColor = health?.is_healthy ? 'var(--green)' : 'var(--amber)'
  const disabled = Boolean(busy)

  return (
    <section className="sp-sec">
      <div className="sp-title"><Database size={12} />Memory</div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
        <span style={{ width: 8, height: 8, borderRadius: '50%', background: statusColor }} />
        <span style={{ fontSize: 12.5, color: 'var(--t2)' }}>
          {health ? (health.is_healthy ? 'Memory database healthy' : 'Memory database needs attention') : 'Checking memory health…'}
        </span>
      </div>
      {health && [...health.issues, ...health.warnings].map((message) => (
        <p key={message} style={{ fontSize: 11.5, lineHeight: 1.5, color: 'var(--amber)', marginBottom: 6 }}>
          {message}
        </p>
      ))}
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 7, marginTop: 10 }}>
        {health?.fts_needs_repair && (
          <button className="cr-btn" disabled={disabled} onClick={() => void repairIndex()} style={{ gridColumn: '1 / -1' }}>
            <Wrench size={12} style={{ marginRight: 5 }} />{busy === 'repair' ? 'Repairing…' : 'Repair Search Index'}
          </button>
        )}
        <button className="cr-btn" disabled={disabled} onClick={() => void exportBackup()}>
          <Download size={12} style={{ marginRight: 5 }} />{busy === 'export' ? 'Exporting…' : 'Export Backup'}
        </button>
        <button className="cr-btn" disabled={disabled} onClick={() => void restoreBackup()}>
          <Upload size={12} style={{ marginRight: 5 }} />{busy === 'restore' ? 'Restoring…' : 'Restore Backup'}
        </button>
        <button className="cr-btn" disabled={disabled || !projectBrief} onClick={() => void loadFacts()}>
          <Eye size={12} style={{ marginRight: 5 }} />{busy === 'facts' ? 'Loading…' : 'View Stored Facts'}
        </button>
        <button className="cr-btn" disabled={disabled || !projectBrief} onClick={() => void clearMemory()} style={{ color: 'var(--red)' }}>
          <Trash2 size={12} style={{ marginRight: 5 }} />{busy === 'clear' ? 'Clearing…' : 'Clear Project Memory'}
        </button>
      </div>
      {facts !== null && (
        <div style={{ maxHeight: 250, overflowY: 'auto', marginTop: 12, display: 'flex', flexDirection: 'column', gap: 7 }}>
          {facts.length === 0 ? (
            <p style={{ fontSize: 12, color: 'var(--t3)' }}>No stored facts for this project.</p>
          ) : facts.map((fact) => (
            <div key={fact.id} style={{ padding: 9, border: '1px solid var(--border)', borderRadius: 8, background: 'var(--bg)' }}>
              <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 5, fontSize: 10.5, color: 'var(--t3)' }}>
                {fact.hard_pinned && <span title="Hard pinned">📌 pinned</span>}
                <span>{fact.category}</span>
                <span>{fact.source_type}</span>
                <span>{fact.importance}</span>
              </div>
              <p style={{ fontSize: 12, lineHeight: 1.5, color: 'var(--t2)', whiteSpace: 'pre-wrap' }}>
                {preview(fact.content)}
              </p>
            </div>
          ))}
        </div>
      )}
    </section>
  )
}
