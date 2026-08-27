/// <reference types="vite/client" />
import { useEffect } from 'react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { loadParticipants } from '@/lib/agents'
import { useIpcListeners } from '@/hooks/useIpcListeners'
import { useAppStore } from '@/stores/useAppStore'
import Sidebar from '@/components/layout/Sidebar'
import EmptyView from '@/components/views/EmptyView'
import SetupView from '@/components/views/SetupView'
import PrimingView from '@/components/views/PrimingView'
import ActiveView from '@/components/views/ActiveView'
import AskUserPopup from '@/components/overlays/AskUserPopup'
import CaptchaOverlay from '@/components/overlays/CaptchaOverlay'
import RateLimitOverlay from '@/components/overlays/RateLimitOverlay'
import Toast from '@/components/shared/Toast'
import DebugPanel from '@/components/shared/DebugPanel'

export default function App() {
  useIpcListeners()

  const { sessionStatus, askUserPending, captchaPending, setRecoveryState } = useAppStore()

  // Batch D: the runtime <style> injection (INJECTED_STYLES) that used to
  // live here has been removed. All those keyframes/classes now live in
  // index.css directly (loaded before first paint, no run-once-guard
  // needed, one fewer place for styles to drift out of sync). Nothing
  // else in this file needs to change for that — every component's
  // `className="ca-xxx"` / `animation: 'ca-xxx ...'` reference is
  // unaffected since the class/keyframe names are identical.

  // Check for recoverable session on startup + load the unified participant
  // registry (built-ins + persisted custom) so every participant consumer and
  // name resolution is populated for the whole session.
  useEffect(() => {
    void loadParticipants()
    invoke<string>('get_recovery_state')
      .then((raw) => {
        const state = JSON.parse(raw) as { available: boolean; session_id: string }
        if (state.available) setRecoveryState(state)
      })
      .catch(console.error)
  }, [setRecoveryState])

  const isActive =
    sessionStatus === 'running' ||
    sessionStatus === 'paused' ||
    sessionStatus === 'complete' ||
    sessionStatus === 'ended'

  return (
    <div className="app-shell">
      <Sidebar />

      <main className="main-shell">
        {sessionStatus === 'idle' && <EmptyView />}
        {sessionStatus === 'setup' && <SetupView />}
        {(sessionStatus === 'priming' || sessionStatus === 'requirements') && <PrimingView />}
        {isActive && <ActiveView />}
      </main>

      {/* Overlays — always mounted, conditionally visible */}
      {askUserPending && <AskUserPopup />}
      {captchaPending && <CaptchaOverlay />}
      <RateLimitOverlay />
      <Toast />

      {import.meta.env.DEV && <DebugPanel />}
    </div>
  )
}
