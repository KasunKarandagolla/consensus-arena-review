import type { DeliveryPhase } from '@/stores/useAppStore'

const steps = [
  { label: 'Intent', phases: ['preparing', 'authoring_acceptance', 'waiting_for_user', 'acceptance_ready'] },
  { label: 'Build', phases: ['implementing', 'repairing'] },
  { label: 'Verify', phases: ['verifying', 'verified'] },
  { label: 'Apply', phases: ['applied'] },
] as const

function stepIndex(phase: DeliveryPhase): number {
  const index = steps.findIndex(step => step.phases.some(value => value === phase))
  return index >= 0 ? index : phase === 'failed' || phase === 'cancelled' ? 0 : 0
}

export default function FounderJourney({ phase }: { phase: DeliveryPhase }) {
  const current = stepIndex(phase)
  const terminal = phase === 'failed' || phase === 'cancelled'
  return <div className="fg" aria-label="Project journey">
    <label className="fl">Project journey</label>
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 6 }}>
      {steps.map((step, index) => {
        const complete = !terminal && index < current
        const active = !terminal && index === current
        return <div key={step.label} style={{ display: 'grid', gap: 5 }}>
          <div style={{ height: 4, borderRadius: 99, background: complete || active ? 'var(--accent)' : 'var(--border)' }} />
          <span style={{ color: active ? 'var(--t1)' : complete ? 'var(--t2)' : 'var(--t3)', fontSize: 12 }}>{step.label}</span>
        </div>
      })}
    </div>
    {terminal && <div style={{ color: 'var(--danger)', fontSize: 12, marginTop: 8 }}>{phase === 'cancelled' ? 'Stopped before completion.' : 'Blocked; review the issue below.'}</div>}
  </div>
}
