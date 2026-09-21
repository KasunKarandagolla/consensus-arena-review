import { useEffect, useState } from 'react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { useAppStore, type DeliveryState, type ProductAuthoritySnapshot } from '@/stores/useAppStore'
import Topbar from '@/components/layout/Topbar'
import FounderJourney from '@/components/shared/FounderJourney'

interface CoordinatorRun {
  run_id: string
  project_id: string
  phase: string
  status: string
  owner_ambiguity_id?: string | null
  owner_question_id?: string | null
  product_review_outcome?: string | null
  pending_owner_decision?: string | null
  terminal_outcome?: string | null
  error?: string | null
}

interface ProductFunctionalState {
  owner_phase: string
  internal_stage: string
  lifecycle: string
  degraded_notices: string[]
  research: {
    owner_directives: number
    mandates_total: number
    active: number
    satisfied: number
    blocked_or_partial: number
    unavailable_channels: string[]
    mandatory_unavailable_channels: string[]
  }
  consultation: {
    request_count: number
    completed: number
    unknown_outcome: number
    blocked_or_recovery: number
    active_request_ids: string[]
    recovery_request_ids: string[]
  }
}

export default function DeliveryView() {
  const state = useAppStore(s => s.deliveryState)
  const setState = useAppStore(s => s.setDeliveryState)
  const [product, setProduct] = useState<ProductAuthoritySnapshot | null>(null)
  const [coordinator, setCoordinator] = useState<CoordinatorRun | null>(null)
  const [functional, setFunctional] = useState<ProductFunctionalState | null>(null)
  const [guidance, setGuidance] = useState('')
  const [consultQuestion, setConsultQuestion] = useState('')
  const [consultProvider, setConsultProvider] = useState<'chatgpt' | 'qwen'>('chatgpt')
  const [ownerActionBusy, setOwnerActionBusy] = useState(false)
  const coordinatorRunId = coordinator?.run_id

  useEffect(() => {
    let disposed = false
    const load = async () => {
      try {
        const raw = await invoke<string>('get_delivery_state')
        if (!disposed) {
          const parsed = JSON.parse(raw) as DeliveryState | null
          if (parsed) setState(parsed)
        }
      } catch {}
      try {
        const raw = await invoke<string>('get_latest_product_os_snapshot')
        if (!disposed) {
          const parsed = JSON.parse(raw) as ProductAuthoritySnapshot | null
          if (parsed) setProduct(parsed)
        }
      } catch {}
      try {
        const raw = await invoke<string>('get_product_coordinator_status', {
          run_id: coordinatorRunId,
        })
        if (!disposed) {
          const parsed = JSON.parse(raw) as CoordinatorRun | null
          if (parsed) setCoordinator(parsed)
        }
      } catch {}
      if (coordinatorRunId) {
        try {
          const raw = await invoke<string>('get_product_functional_state', {
            run_id: coordinatorRunId,
          })
          if (!disposed) {
            const parsed = JSON.parse(raw) as ProductFunctionalState
            setFunctional(parsed)
          }
        } catch {}
      }
    }
    void load()
    const timer = window.setInterval(() => void load(), 2500)
    return () => {
      disposed = true
      window.clearInterval(timer)
    }
  }, [coordinatorRunId, setState])

  const terminal =
    ['verified', 'applied', 'cancelled', 'failed'].includes(state?.phase ?? '') ||
    ['completed', 'stopped', 'pivoted', 'blocked', 'cancelled'].includes(coordinator?.status ?? '')
  const canResume =
    ['failed', 'blocked', 'reconciling'].includes(coordinator?.status ?? '') ||
    ['cancelled', 'failed'].includes(state?.phase ?? '')

  async function apply() {
    try {
      await invoke('apply_delivery', { session_id: state?.session_id })
    } catch (error) {
      useAppStore.getState().addToast(String(error), 7000)
    }
  }

  async function resume() {
    try {
      if (
        coordinator &&
        !['completed', 'stopped', 'pivoted', 'cancelled'].includes(coordinator.status)
      ) {
        await invoke('resume_product_project', { run_id: coordinator.run_id })
      } else {
        await invoke('resume_delivery')
      }
    } catch (error) {
      useAppStore.getState().addToast(String(error), 7000)
    }
  }

  async function abort() {
    try {
      if (
        coordinator &&
        !['completed', 'stopped', 'pivoted', 'cancelled'].includes(coordinator.status)
      ) {
        await invoke('cancel_product_project', {
          run_id: coordinator.run_id,
          reason: 'Stopped by owner',
        })
      } else if (state) {
        await invoke('abort_delivery', { session_id: state.session_id })
      }
    } catch (error) {
      useAppStore.getState().addToast(String(error), 7000)
    }
  }

  async function answerOwner(option: string) {
    if (!coordinator) return
    try {
      await invoke('answer_product_question', {
        run_id: coordinator.run_id,
        selected_option: option,
      })
    } catch (error) {
      useAppStore.getState().addToast(String(error), 7000)
    }
  }

  async function injectGuidance() {
    if (!coordinator || !guidance.trim() || ownerActionBusy) return
    setOwnerActionBusy(true)
    try {
      const raw = await invoke<string>('inject_product_guidance', {
        run_id: coordinator.run_id,
        guidance: guidance.trim(),
      })
      setCoordinator(JSON.parse(raw) as CoordinatorRun)
      setGuidance('')
      useAppStore.getState().addToast('Guidance accepted. Arena is re-planning safely.', 5000)
    } catch (error) {
      useAppStore.getState().addToast(String(error), 7000)
    } finally {
      setOwnerActionBusy(false)
    }
  }

  async function requestConsultation() {
    if (!coordinator || !consultQuestion.trim() || ownerActionBusy) return
    setOwnerActionBusy(true)
    try {
      const raw = await invoke<string>('request_product_consultation', {
        run_id: coordinator.run_id,
        provider: consultProvider,
        question: consultQuestion.trim(),
      })
      setCoordinator(JSON.parse(raw) as CoordinatorRun)
      setConsultQuestion('')
      useAppStore.getState().addToast('Consultation completed or entered safe recovery state.', 6000)
    } catch (error) {
      useAppStore.getState().addToast(String(error), 7000)
    } finally {
      setOwnerActionBusy(false)
    }
  }

  async function recoverConsultation(requestId: string, action: 'observe' | 'abandon') {
    if (!coordinator || ownerActionBusy) return
    setOwnerActionBusy(true)
    try {
      const raw = await invoke<string>('recover_product_consultation', {
        run_id: coordinator.run_id,
        request_id: requestId,
        action,
      })
      setCoordinator(JSON.parse(raw) as CoordinatorRun)
    } catch (error) {
      useAppStore.getState().addToast(String(error), 7000)
    } finally {
      setOwnerActionBusy(false)
    }
  }

  return (
    <section className="view">
      <Topbar title={state?.objective?.slice(0, 48) || 'Build'} />
      <div className="scroll pt" style={{ display: 'flex', justifyContent: 'center' }}>
        <div className="fw">
          <div className="fh">
            {state?.status_text ||
              (coordinator?.status === 'waiting_for_owner'
                ? 'Waiting for your decision…'
                : ['stopped', 'pivoted'].includes(coordinator?.status ?? '')
                  ? `Product work ${coordinator?.status}`
                  : coordinator?.status === 'blocked'
                    ? 'Arena is blocked pending the next bounded action'
                    : coordinator?.status === 'completed'
                      ? 'Product work complete'
                  : coordinator?.status === 'failed'
                    ? 'Arena is blocked'
                    : 'Arena is progressing your project…')}
          </div>
          <div className="fh-sub">Discover → Decide → Deliver → Release</div>
          <div className="fh-sub">
            Arena owns product decisions, acceptance, and verification; worker prose never marks a
            build successful.
          </div>

          {coordinator && (
            <div className="fg">
              <label className="fl">Arena progress</label>
              <div style={{ color: 'var(--t2)', fontSize: 13 }}>
                {coordinator.phase.replace(/_/g, ' ')} · {coordinator.status.replace(/_/g, ' ')}
              </div>
              {functional && (
                <div style={{ color: 'var(--t3)', fontSize: 12, marginTop: 4 }}>
                  {functional.owner_phase} · {functional.internal_stage.replace(/_/g, ' ')} ·{' '}
                  {functional.lifecycle.replace(/_/g, ' ')}
                  {functional.research.mandates_total > 0
                    ? ` · research ${functional.research.satisfied}/${functional.research.mandates_total}`
                    : ''}
                </div>
              )}
              {functional?.degraded_notices.map(notice => (
                <div key={notice} className="form-error" role="status" style={{ marginTop: 8 }}>
                  {notice}
                </div>
              ))}
              {coordinator.error && (
                <div className="form-error" role="status">
                  {coordinator.error}
                </div>
              )}
              {coordinator.status === 'waiting_for_owner' &&
                coordinator.pending_owner_decision !== 'approve_apply' && (
                <div className="form-error" role="status">
                  {product?.records.ambiguities.find(
                    item => item.ambiguity_id === coordinator.owner_ambiguity_id,
                  )?.question || 'Arena needs your product direction.'}
                  <div style={{ display: 'flex', gap: 8, marginTop: 10 }}>
                    <button
                      className="btn-p"
                      onClick={() =>
                        void answerOwner(
                          coordinator.pending_owner_decision === 'authorize_validation_experiment'
                            ? 'authorize_validation_experiment'
                            : coordinator.pending_owner_decision === 'authorize_build'
                              ? 'authorize_build'
                              : ['stop_run', 'pivot_run', 'continue_evaluation'].includes(
                                    coordinator.pending_owner_decision ?? '',
                                  )
                                ? 'continue_evaluation'
                                : 'authorize_narrow_build',
                        )
                      }
                    >
                      {coordinator.pending_owner_decision === 'authorize_validation_experiment'
                        ? 'Run bounded validation experiment'
                        : coordinator.pending_owner_decision === 'authorize_build'
                          ? 'Continue after owner decision'
                          : ['stop_run', 'pivot_run', 'continue_evaluation'].includes(
                                coordinator.pending_owner_decision ?? '',
                              )
                            ? 'Continue evaluation'
                            : 'Proceed with bounded build'}
                    </button>
                    <button className="sv-btn" onClick={() => void answerOwner('stop')}>
                      Stop
                    </button>
                    <button className="sv-btn" onClick={() => void answerOwner('pivot')}>
                      Pivot
                    </button>
                  </div>
                </div>
              )}
              {coordinator.status === 'waiting_for_owner' &&
                coordinator.pending_owner_decision === 'approve_apply' && (
                  <div className="form-success" role="status">
                    The exact candidate is Verified. Review the result, then use Apply below to
                    perform the explicit Safe Apply.
                  </div>
                )}
            </div>
          )}

          {coordinator &&
            !['completed', 'stopped', 'pivoted', 'cancelled'].includes(coordinator.status) && (
              <details className="fg">
                <summary style={{ cursor: 'pointer', color: 'var(--t2)', fontSize: 13 }}>
                  Owner controls
                </summary>
                <div style={{ marginTop: 12, display: 'grid', gap: 12 }}>
                  <div>
                    <label className="fl" htmlFor="arena-guidance">
                      Change direction or add a constraint
                    </label>
                    <textarea
                      id="arena-guidance"
                      value={guidance}
                      onChange={event => setGuidance(event.target.value)}
                      placeholder="Example: Research YouTube and GitHub first before deciding."
                      rows={3}
                      maxLength={8000}
                      style={{ width: '100%', resize: 'vertical' }}
                    />
                    <button
                      className="sv-btn"
                      disabled={ownerActionBusy || !guidance.trim()}
                      onClick={() => void injectGuidance()}
                      style={{ marginTop: 8 }}
                    >
                      Update Arena
                    </button>
                  </div>

                  <div>
                    <label className="fl" htmlFor="arena-consult-question">
                      Optional frontier consultation
                    </label>
                    <div style={{ color: 'var(--t3)', fontSize: 12, marginBottom: 6 }}>
                      Advisory only. It cannot mark gates PASS, Verify a candidate, or Apply.
                    </div>
                    <select
                      value={consultProvider}
                      onChange={event =>
                        setConsultProvider(event.target.value === 'qwen' ? 'qwen' : 'chatgpt')
                      }
                      disabled={ownerActionBusy}
                      style={{ marginBottom: 8 }}
                    >
                      <option value="chatgpt">ChatGPT</option>
                      <option value="qwen">Qwen</option>
                    </select>
                    <textarea
                      id="arena-consult-question"
                      value={consultQuestion}
                      onChange={event => setConsultQuestion(event.target.value)}
                      placeholder="Ask for an independent opinion on a consequential unresolved decision."
                      rows={3}
                      maxLength={8000}
                      style={{ width: '100%', resize: 'vertical' }}
                    />
                    <button
                      className="sv-btn"
                      disabled={ownerActionBusy || !consultQuestion.trim()}
                      onClick={() => void requestConsultation()}
                      style={{ marginTop: 8 }}
                    >
                      Consult
                    </button>
                  </div>

                  {!!functional?.consultation.recovery_request_ids.length && (
                    <div className="form-error" role="status">
                      A consultation may have been submitted before Arena lost certainty. Automatic
                      resend is disabled.
                      {functional.consultation.recovery_request_ids.map(requestId => (
                        <div
                          key={requestId}
                          style={{ display: 'flex', gap: 8, marginTop: 8, flexWrap: 'wrap' }}
                        >
                          <button
                            className="sv-btn"
                            disabled={ownerActionBusy}
                            onClick={() => void recoverConsultation(requestId, 'observe')}
                          >
                            Observe existing conversation
                          </button>
                          <button
                            className="sv-btn"
                            disabled={ownerActionBusy}
                            onClick={() => void recoverConsultation(requestId, 'abandon')}
                          >
                            Abandon unresolved consultation
                          </button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </details>
            )}

          {state && <FounderJourney phase={state.phase} />}

          {product && (
            <div className="fg">
              <label className="fl">What Arena learned</label>
              <div style={{ color: 'var(--t2)', fontSize: 13 }}>
                Project revision {product.records.project_revision} ·{' '}
                {product.records.evidence.length
                  ? `${product.records.evidence.length} evidence record(s)`
                  : 'No evidence yet'}
              </div>
              {product.records.evidence.slice(-3).map(item => (
                <div key={item.evidence_id} style={{ padding: '8px 0', color: 'var(--t2)' }}>
                  {item.verification === 'independently_verified'
                    ? '✓ Independently verified'
                    : item.verification === 'contradicted'
                      ? 'Blocked: contradicted'
                      : item.verification === 'unresolved'
                        ? 'Blocked: unresolved'
                        : 'Unverified proposal'}{' '}
                  — {item.summary}
                  {item.source?.title ? ` · ${item.source.title}` : ''}
                </div>
              ))}
              {product.records.ambiguities
                .filter(item => item.resolver === 'owner' && item.status !== 'resolved')
                .map(item => (
                  <div key={item.ambiguity_id} className="form-error" role="status">
                    Owner decision required: {item.question}
                  </div>
                ))}
              {product.research_gate && (
                <div style={{ color: 'var(--t3)', fontSize: 12 }}>
                  Research gate: {product.research_gate.status} — {product.research_gate.reason}
                </div>
              )}
            </div>
          )}

          {state?.build_package_id && (
            <div className="fg">
              <label className="fl">Build Package</label>
              <div style={{ color: 'var(--t2)', fontSize: 13 }}>
                {state.build_package_ready
                  ? 'Accepted package is bound to Delivery.'
                  : 'Build Package is not current; Delivery is blocked.'}
              </div>
            </div>
          )}

          {!!state?.contract?.acceptance_criteria?.length && (
            <div className="fg">
              <label className="fl">This build will be considered successful when:</label>
              {state.contract.acceptance_criteria.map(c => (
                <div key={c.id} style={{ padding: '8px 0', color: 'var(--t2)' }}>
                  ✓ {c.description}
                </div>
              ))}
            </div>
          )}

          {state?.work_order?.error && (
            <div className="form-error" role="status">
              {state.work_order.error}
            </div>
          )}
          {!!state?.evidence?.length && (
            <div className="fg">
              <label className="fl">Evidence</label>
              <div style={{ color: 'var(--t2)', fontSize: 13 }}>
                {state.evidence[state.evidence.length - 1].summary}
              </div>
            </div>
          )}
          {state && (
            <div className="fg" style={{ color: 'var(--t3)', fontSize: 12 }}>
              Attempt {state.attempt} · {state.branch_name || 'isolated candidate'}
              {state.verification_summary ? ` · ${state.verification_summary}` : ''}
            </div>
          )}
          {state?.phase === 'verified' && (
            <button className="btn-p" onClick={() => void apply()}>
              Apply
            </button>
          )}
          {state?.phase === 'applied' && (
            <div className="form-success">Candidate applied safely to the original branch.</div>
          )}
          {canResume && state?.phase !== 'verified' && state?.phase !== 'applied' && (
            <button className="btn-p" onClick={() => void resume()}>
              Resume build
            </button>
          )}
          {!terminal && (
            <button className="sv-btn" onClick={() => void abort()}>
              Stop build
            </button>
          )}
        </div>
      </div>
    </section>
  )
}
