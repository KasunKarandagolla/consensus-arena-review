import type { ReactNode } from 'react'
import { PanelLeft } from 'lucide-react'
import { useAppStore } from '@/stores/useAppStore'

interface Props { title?: string; titleBadge?: ReactNode; right?: ReactNode }

export default function Topbar({ title = 'Consensus Arena', titleBadge, right }: Props) {
  const toggleSidebar = useAppStore(s => s.toggleSidebar)
  return (
    <header className="topbar">
      <div className="tb-left">
        <button className="ic-btn" onClick={toggleSidebar} aria-label="Toggle sidebar"><PanelLeft size={18}/></button>
        <span className="tb-title">{title}</span>{titleBadge}
      </div>
      <div className="tb-right">{right}</div>
    </header>
  )
}
