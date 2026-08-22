/// <reference types="vite/client" />
import { useEffect, useRef, useState } from 'react'
import { safeListen as listen } from '@/lib/tauri'
import { X, Trash2 } from 'lucide-react'

interface LogEntry {
  id: number
  time: Date
  tag: string
  message: string
  level: string
}

const MAX_ENTRIES = 200

export default function DebugPanel() {
  // RISK-DEVLEAK: double-guarded — App.tsx wraps with {DEV && <DebugPanel />}
  if (!import.meta.env.DEV) return null

  const [visible, setVisible] = useState(false)
  const [entries, setEntries] = useState<LogEntry[]>([])
  const [filter, setFilter] = useState('')
  const counter = useRef(0)
  const logEndRef = useRef<HTMLDivElement>(null)

  function addEntry(entry: Omit<LogEntry, 'id'>) {
    setEntries((prev) => {
      const next = [...prev, { ...entry, id: ++counter.current }]
      return next.length > MAX_ENTRIES ? next.slice(next.length - MAX_ENTRIES) : next
    })
  }

  // Ctrl+Shift+D toggle
  useEffect(() => {
    function handleKey(e: KeyboardEvent) {
      if (e.ctrlKey && e.shiftKey && e.key === 'D') {
        e.preventDefault()
        setVisible((v) => !v)
      }
    }
    document.addEventListener('keydown', handleKey)
    return () => document.removeEventListener('keydown', handleKey)
  }, [])

  // debug-log event listener
  useEffect(() => {
    let unsub: (() => void) | null = null
    listen<{ tag: string; message: string; level: string }>('debug-log', (e) => {
      addEntry({ time: new Date(), ...e.payload })
    })
      .then((u) => { unsub = u })
      .catch(console.error)
    return () => { unsub?.() }
  }, [])

  // Auto-scroll to bottom
  useEffect(() => {
    if (visible) {
      logEndRef.current?.scrollIntoView({ behavior: 'smooth' })
    }
  }, [entries, visible])

  if (!visible) return null

  const filtered = filter
    ? entries.filter(
        (e) =>
          e.tag.toLowerCase().includes(filter.toLowerCase()) ||
          e.message.toLowerCase().includes(filter.toLowerCase()),
      )
    : entries

  function fmtTime(d: Date): string {
    return d.toTimeString().slice(0, 8)
  }

  function rowColor(level: string, tag: string): string {
    if (tag === 'error' || level === 'error') return 'rgba(239,68,68,0.85)'
    if (tag === 'debug' || level === 'debug') return 'var(--text-muted)'
    return 'var(--text-secondary)'
  }

  return (
    <div
      style={{
        position: 'fixed',
        bottom: 16,
        right: 16,
        width: 500,
        height: 340,
        zIndex: 8888,
        background: 'rgba(10,10,16,0.92)',
        border: '1px solid rgba(255,255,255,0.1)',
        borderRadius: 'var(--radius-lg,16px)',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'hidden',
        backdropFilter: 'blur(8px)',
        boxShadow: '0 20px 60px rgba(0,0,0,0.5)',
        fontFamily: 'var(--font-mono)',
      }}
    >
      {/* Title bar */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '8px 12px',
          borderBottom: '1px solid rgba(255,255,255,0.08)',
          flexShrink: 0,
        }}
      >
        <span
          style={{
            fontSize: 11,
            fontWeight: 700,
            color: 'rgba(255,255,255,0.7)',
            letterSpacing: '0.08em',
            textTransform: 'uppercase',
            flex: 1,
          }}
        >
          Debug Log
        </span>

        {/* Filter */}
        <input
          type="text"
          placeholder="filter tag…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          style={{
            background: 'rgba(255,255,255,0.06)',
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: 5,
            padding: '3px 8px',
            fontSize: 11,
            color: 'rgba(255,255,255,0.65)',
            fontFamily: 'var(--font-mono)',
            outline: 'none',
            width: 100,
          }}
        />

        <button
          onClick={() => setEntries([])}
          title="Clear"
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            color: 'rgba(255,255,255,0.4)',
            display: 'flex',
            alignItems: 'center',
            padding: 3,
            borderRadius: 4,
            transition: 'color 0.12s',
          }}
          onMouseEnter={(e) =>
            (e.currentTarget.style.color = 'rgba(255,255,255,0.7)')
          }
          onMouseLeave={(e) =>
            (e.currentTarget.style.color = 'rgba(255,255,255,0.4)')
          }
        >
          <Trash2 size={12} />
        </button>

        <button
          onClick={() => setVisible(false)}
          title="Close (Ctrl+Shift+D)"
          style={{
            background: 'none',
            border: 'none',
            cursor: 'pointer',
            color: 'rgba(255,255,255,0.4)',
            display: 'flex',
            alignItems: 'center',
            padding: 3,
            borderRadius: 4,
            transition: 'color 0.12s',
          }}
          onMouseEnter={(e) =>
            (e.currentTarget.style.color = 'rgba(255,255,255,0.7)')
          }
          onMouseLeave={(e) =>
            (e.currentTarget.style.color = 'rgba(255,255,255,0.4)')
          }
        >
          <X size={12} />
        </button>
      </div>

      {/* Log list */}
      <div
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: '6px 0',
        }}
      >
        {filtered.length === 0 && (
          <div
            style={{
              padding: '20px 16px',
              fontSize: 11,
              color: 'rgba(255,255,255,0.25)',
              textAlign: 'center',
            }}
          >
            {filter ? 'No entries matching filter' : 'Waiting for debug events…'}
          </div>
        )}
        {filtered.map((entry) => (
          <div
            key={entry.id}
            style={{
              display: 'flex',
              gap: 10,
              padding: '2px 12px',
              fontSize: 11.5,
              lineHeight: 1.5,
              fontFamily: 'var(--font-mono)',
              color: rowColor(entry.level, entry.tag),
            }}
          >
            <span style={{ color: 'rgba(255,255,255,0.2)', flexShrink: 0 }}>
              {fmtTime(entry.time)}
            </span>
            <span
              style={{
                color: 'rgba(99,102,241,0.8)',
                flexShrink: 0,
                minWidth: 70,
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
              }}
            >
              [{entry.tag}]
            </span>
            <span
              style={{
                flex: 1,
                wordBreak: 'break-all',
                whiteSpace: 'pre-wrap',
              }}
            >
              {entry.message}
            </span>
          </div>
        ))}
        <div ref={logEndRef} />
      </div>

      {/* Footer */}
      <div
        style={{
          padding: '4px 12px',
          borderTop: '1px solid rgba(255,255,255,0.06)',
          fontSize: 10,
          color: 'rgba(255,255,255,0.2)',
          display: 'flex',
          justifyContent: 'space-between',
          flexShrink: 0,
        }}
      >
        <span>{filtered.length} entries</span>
        <span>Ctrl+Shift+D to toggle</span>
      </div>
    </div>
  )
}
