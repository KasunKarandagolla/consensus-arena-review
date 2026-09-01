import { useEffect } from 'react'
import { safeListen as listen } from '@/lib/tauri'
import { displayName } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'

export function useIpcListeners() {
  useEffect(() => {
    const cleanups: Array<() => void> = []
    let disposed = false
    const browserDiagnosticToastTimes = new Map<string, number>()

    function addBrowserDiagnosticToast(
      agentId: string,
      phase: string,
      message: string,
      error: string | null,
      text: string,
      duration: number,
    ) {
      const key = [agentId, phase, message, error || ''].join('\x1f')
      const now = Date.now()
      const lastShownAt = browserDiagnosticToastTimes.get(key) || 0
      if (now - lastShownAt < 10000) return
      browserDiagnosticToastTimes.set(key, now)
      useAppStore.getState().addToast(text, duration)
    }

    async function setup() {
      const store = useAppStore.getState()

      // session-status
      cleanups.push(await listen('session-status', (e) => {
        const { status, setup_order, selected_agent_ids } = e.payload as {
          status: string
          setup_order?: string[]
          selected_agent_ids?: string[]
        }
        store.setSessionStatus(status as Parameters<typeof store.setSessionStatus>[0])
        if (status === 'setup') {
          store.setSessionAgentIds(
            Array.isArray(setup_order)
              ? setup_order
              : Array.isArray(selected_agent_ids)
                ? selected_agent_ids
                : [],
          )
          store.clearSetupProgress()
          store.setSetupReadyAgentId(null)
          store.setSetupFailedAgentId(null)
          store.setActiveAgentId(null)
          store.setActiveTurn(null, null)
          store.setCurrentAgentResponse('')
          store.setLiveStatusText('')
          store.setAskUserPending(null)
          store.setCaptchaPending(null)
          store.setRateLimitPending(null)
        }
        if (status === 'ended' || status === 'complete' || status === 'idle') {
          store.setSetupReadyAgentId(null)
          store.setActiveAgentId(null)
          if (status === 'ended' && useAppStore.getState().liveStatusText === 'Session complete') {
            store.setLiveStatusText('')
          }
        }
      }))

      // setup-agent-ready
      cleanups.push(await listen('setup-agent-ready', (e) => {
        const { agent_id } = e.payload as { agent_id: string }
        store.setSetupReadyAgentId(agent_id)
        store.setSetupFailedAgentId(null)
        store.setLiveStatusText(`${displayName(agent_id)} priming prompt is ready — press Send in the model window`)
      }))

      // setup-agent-complete
      cleanups.push(await listen('setup-agent-complete', (e) => {
        const { agent_id } = e.payload as { agent_id: string; conversation_url: string }
        store.addSetupProgress(agent_id)
        if (useAppStore.getState().setupReadyAgentId === agent_id) store.setSetupReadyAgentId(null)
        if (useAppStore.getState().setupFailedAgentId === agent_id) store.setSetupFailedAgentId(null)
        store.setLiveStatusText(`${displayName(agent_id)} primed`)
      }))

      cleanups.push(await listen('setup-agent-failed', (e) => {
        const { agent_id } = e.payload as { agent_id: string | null }
        if (!agent_id) return
        store.setSetupReadyAgentId(null)
        store.setSetupFailedAgentId(agent_id)
        store.setLiveStatusText(`${displayName(agent_id)} needs attention. Complete login/loading/security checks in the model window, then retry setup.`)
      }))

      // setup-complete
      cleanups.push(await listen('setup-complete', (_e) => {
        store.setSessionStatus('running')
        store.setSetupReadyAgentId(null)
        store.setSetupFailedAgentId(null)
        store.setLiveStatusText('')
      }))

      // agent-state-change
      cleanups.push(await listen('agent-state-change', (e) => {
        const { agent_id, state: agentState } = e.payload as {
          agent_id: string; state: string; response?: string; tokens?: number
        }
        store.setActiveAgentId(agent_id)
        store.setLiveStatusText(`${agent_id} is ${agentState}...`)
        if (e.payload && typeof (e.payload as { response?: string }).response === 'string' && (e.payload as { response: string }).response) {
          store.setCurrentAgentResponse((e.payload as { response: string }).response)
        }
      }))

      cleanups.push(await listen('active-turn-state', (e) => {
        const { event, agent_id, turn_number } = e.payload as {
          event: string; agent_id: string; turn_number: number
        }
        if (event === 'active_turn_started' || event === 'active_prompt_injected') {
          store.setActiveTurn(agent_id, turn_number)
          store.setLiveStatusText(`${displayName(agent_id)} prompt was inserted…`)
        }
        if (event === 'active_prompt_submitted') {
          store.setActiveTurn(agent_id, turn_number)
          store.setLiveStatusText(`${displayName(agent_id)} prompt was auto-submitted. Waiting for response…`)
        }
        if (event === 'active_waiting_for_response') {
          store.setActiveTurn(agent_id, turn_number)
          store.setLiveStatusText(`${displayName(agent_id)} is waiting for a response…`)
        }
        if (event === 'active_submit_failed') {
          store.setActiveTurn(agent_id, turn_number)
          store.setLiveStatusText(`${displayName(agent_id)} prompt was inserted but not submitted. Click Send in the model window or paste the response.`)
        }
        if (event === 'active_response_captured') {
          store.setActiveTurn(null, null)
          store.setActiveAgentId(agent_id)
          store.setLiveStatusText(`${displayName(agent_id)} response captured`)
        }
        if (event === 'active_turn_timeout') {
          store.setActiveTurn(agent_id, turn_number)
          store.setLiveStatusText(`${displayName(agent_id)} response was not captured. Paste the visible response to continue.`)
        }
      }))

      // agent-routing
      cleanups.push(await listen('agent-routing', (e) => {
        const { from_model, to_model } = e.payload as {
          from_model: string; to_model: string; reason?: string
        }
        store.setActiveAgentId(to_model)
        store.setLiveStatusText(`Routing from ${from_model} to ${to_model}...`)
      }))

      // boss-message
      cleanups.push(await listen('boss-message', (e) => {
        const { text } = e.payload as { text: string; message_type: string }
        store.setLiveStatusText(text)
      }))

      // browser-diagnostic
      cleanups.push(await listen('browser-diagnostic', (e) => {
        const { agent_id, window_label, phase, url, message, error } = e.payload as {
          agent_id: string
          window_label: string
          phase: string
          url: string
          message: string
          error: string | null
        }
        const name = displayName(agent_id)
        if (phase === 'captcha_or_challenge') {
          const detail = error || message
          store.setLiveStatusText(`${name} needs verification. Complete it in the model window, then click Resume.`)
          store.setCaptchaPending({ agent_id })
          addBrowserDiagnosticToast(agent_id, phase, message, error, `${name} needs verification: ${detail}`, 7000)
        } else if (phase === 'unshowable_url') {
          store.setLiveStatusText(`${name} navigated to a URL this WebView cannot display.`)
          addBrowserDiagnosticToast(agent_id, phase, message, error, `${name} navigated to a URL this WebView cannot display. Try completing verification, then Resume. See Settings → Diagnostics.`, 7000)
        } else if (phase === 'navigation_error') {
          const detail = error || message
          store.setLiveStatusText(`${name} navigation blocked: ${detail}`)
          addBrowserDiagnosticToast(agent_id, phase, message, error, `${name} window (${window_label}) navigation issue at ${url}: ${detail}`, 7000)
        } else if (phase === 'setup_failed_recoverable') {
          store.setSetupFailedAgentId(agent_id)
          store.setLiveStatusText(`${name} needs attention. Complete login/loading/security checks, then retry setup.`)
        } else if (phase === 'error' || error) {
          const detail = error || message
          const noComposer = detail.includes('loaded but no composer was detected')
          store.setLiveStatusText(noComposer ? detail : `${name} window error: ${detail}`)
          const text = noComposer
            ? detail
            : detail.includes('timed out waiting for readiness')
            ? `${name} window (${window_label}) timed out. ${detail}`
            : `${name} window (${window_label}) failed to load ${url}: ${detail}`
          addBrowserDiagnosticToast(agent_id, phase, message, error, text, 7000)
        } else if (phase === 'creating' || phase === 'loading') {
          store.setLiveStatusText(
            message.includes('Page load finished')
              ? `${name} window loaded. Checking for a composer…`
              : `${name} window loading ${url}...`,
          )
        } else if (phase === 'waiting_user_send') {
          store.setLiveStatusText(`${name} window is ready — press Send in the model window`)
        }
      }))

      // blueprint-section-added
      cleanups.push(await listen('blueprint-section-added', (e) => {
        const { section_id, title, content } = e.payload as {
          section_id: string; title: string; content: string
        }
        store.upsertBlueprintSection({
          id: section_id,
          title,
          content,
          status: 'agreed',
        })
      }))

      // blueprint-update
      cleanups.push(await listen('blueprint-update', (e) => {
        const { section_id, title, content, status } = e.payload as {
          section_id: string; title: string; content: string
          status: 'draft' | 'agreed' | 'negotiation' | 'disputed'
        }
        store.upsertBlueprintSection({ id: section_id, title, content, status })
      }))

      // agent-message
      cleanups.push(await listen('agent-message', (e) => {
        const { response } = e.payload as { response: string }
        if (response) store.setCurrentAgentResponse(response)
      }))

      // agent-ask-user
      cleanups.push(await listen('agent-ask-user', (e) => {
        const payload = e.payload as {
          question: string; options: string[]; allow_custom: boolean
        }
        store.setAskUserPending(payload)
      }))

      // captcha-detected
      cleanups.push(await listen('captcha-detected', (e) => {
        const { agent_id } = e.payload as { agent_id: string }
        store.setCaptchaPending({ agent_id })
        store.setLiveStatusText(`${displayName(agent_id)} needs verification. Complete it in the model window, then click Resume.`)
      }))

      // rate-limit-reached
      cleanups.push(await listen('rate-limit-reached', (e) => {
        const { agent_id, estimated_reset_mins } = e.payload as {
          agent_id: string; estimated_reset_mins: number
        }
        store.setRateLimitPending({ agent_id, estimated_reset_mins })
      }))

      // session-checkpoint
      cleanups.push(await listen('session-checkpoint', (_e) => {
        store.addToast('Progress saved')
      }))

      // memory-updated
      cleanups.push(await listen('memory-updated', (e) => {
        const { memory_type, trigger } = e.payload as {
          memory_type: 'session' | 'project' | 'global'
          trigger: 'routing' | 'route_compare' | 'blueprint' | 'user_answer' | 'session_complete'
        }
        console.debug(`[memory] ${memory_type} updated by ${trigger}`)
      }))

      // memory-health-warning
      cleanups.push(await listen('memory-health-warning', (e) => {
        const { text, fts_needs_repair } = e.payload as {
          text: string; fts_needs_repair: boolean
        }
        store.addToast(fts_needs_repair ? `${text} Repair is available in Settings.` : text, 5000)
      }))

      // brain-status
      cleanups.push(await listen('brain-status', (e) => {
        const { active, model } = e.payload as { active: string; model: string }
        store.setActiveBrain({ kind: active as import('@/stores/useAppStore').ActiveBrainKind, model: model || '' })
      }))

      // session-complete
      cleanups.push(await listen('session-complete', (_e) => {
        store.setSessionStatus('complete')
        store.setLiveStatusText('Session complete')
      }))
    }

    setup().then(() => {
      if (disposed) cleanups.splice(0).forEach((cleanup) => cleanup())
    }).catch(console.error)

    return () => {
      disposed = true
      cleanups.splice(0).forEach((cleanup) => cleanup())
    }
  }, [])
}
