import type { ReactNode } from 'react'
import { PanelLeft, Cpu } from 'lucide-react'
import { useAppStore } from '@/stores/useAppStore'

interface Props { title?: string; titleBadge?: ReactNode; right?: ReactNode }

export default function Topbar({ title = 'Consensus Arena', titleBadge, right }: Props) {
  const toggleSidebar = useAppStore(s => s.toggleSidebar)
  const activeBrain = useAppStore(s => s.activeBrain)
  const sessionStatus = useAppStore(s => s.sessionStatus)
  const showBrain = (sessionStatus === 'running' || sessionStatus === 'complete' || sessionStatus === 'ended') && activeBrain.kind !== 'unknown'
  const brainLabel = (() => {
    if (!showBrain) return null
    if (activeBrain.kind === 'unavailable') return 'Brain unavailable'
    if (!activeBrain.model) return null
    // Show "Powered by <model>" — truncate long model names
    const name = activeBrain.model.length > 28 ? activeBrain.model.slice(0, 28) + '…' : activeBrain.model
    if (activeBrain.kind === 'fallback') return `Powered by ${name} (fallback)`
    if (activeBrain.kind === 'secondary') return `Powered by ${name} (secondary)`
    return `Powered by ${name}`
  })()
  return (
    <header className="topbar">
      <div className="tb-left">
        <button className="ic-btn" onClick={toggleSidebar} aria-label="Toggle sidebar"><PanelLeft size={18}/></button>
        <span className="tb-title">{title}</span>{titleBadge}
        {brainLabel && <span className="tb-brain" title={activeBrain.model} style={{ display: 'inline-flex', alignItems: 'center', gap: 6, fontSize: 11.5, fontWeight: 500, color: 'var(--t2)', background: 'var(--surface2)', border: '1px solid var(--border)', borderRadius: 20, padding: '3px 10px', marginLeft: 10 }}><Cpu size={12} />{brainLabel}</span>}
      </div>
      <div className="tb-right">{right}</div>
    </header>
  )
}
