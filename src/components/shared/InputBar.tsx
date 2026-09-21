import { useEffect, useRef, useState } from 'react'
import { ArrowUp, Plus, Square, RotateCcw } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { useAppStore } from '@/stores/useAppStore'

export default function InputBar() {
  const { sessionStatus, setupBrief, setSetupBrief, setSessionStatus, addToast, selectedSessionId } = useAppStore()
  const [activeText, setActiveText] = useState('')
  const [pausing, setPausing] = useState(false)
  const [resuming, setResuming] = useState(false)
  const ref = useRef<HTMLTextAreaElement>(null)
  const idle = sessionStatus === 'idle'
  const running = sessionStatus === 'running'
  const paused = sessionStatus === 'paused'
  const active = running || paused
  const disabled = !idle && !active
  const text = idle ? setupBrief : activeText

  useEffect(() => { if (ref.current) { ref.current.style.height = 'auto'; ref.current.style.height = `${Math.min(ref.current.scrollHeight, 180)}px` } }, [text])
  // Reset transient flags when backend confirms status change
  useEffect(() => { if (sessionStatus === 'paused') setPausing(false); if (sessionStatus === 'running') setResuming(false) }, [sessionStatus])

  async function submit() {
    const value = text.trim()
    if (!value) return
    if (idle) { setSetupBrief(value); setSessionStatus('setup'); return }
    try { await invoke('user_input', { text: value }); setActiveText(''); addToast('Context sent to leader') }
    catch (error) { console.error(error); addToast('Could not send context') }
  }

  async function requestPause() {
    if (pausing) return
    setPausing(true)
    try {
      // Graceful pause: finish current atomic movement, persist checkpoint, then Paused
      await invoke('pause_session')
      addToast('Pausing — finishing current step…')
      // Status will flip to paused via session-status event after backend persists checkpoint
    } catch (error) {
      console.error(error)
      addToast('Could not pause session')
      setPausing(false)
    }
  }

  async function resume() {
    if (resuming) return
    setResuming(true)
    try {
      await invoke('resume_session')
      addToast('Resuming from checkpoint…')
    } catch (error) {
      console.error(error)
      addToast(String(error).includes('already') ? 'Resume already in progress' : 'Could not resume session')
      setResuming(false)
    }
  }

  async function hardAbort() {
    try { await invoke('abort_session'); addToast('Session stopped') } catch (error) { console.error(error); addToast('Could not stop session') }
  }

  return (
    <div className="izone">
      <div className={`ibox${disabled ? ' disabled' : ''}`}>
        <textarea ref={ref} rows={2} disabled={disabled} value={text}
          placeholder={idle ? 'What are we building? Be as specific as you like.' : paused ? 'Session paused — press Resume to continue or steer with context…' : running ? 'Steer the session or add context…' : 'Waiting for setup to complete...'}
          onChange={e => idle ? setSetupBrief(e.target.value) : setActiveText(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void submit() } }}/>
        <div className="irow">
          <div className="irow-left">
            <button className="i-plus" disabled aria-label="Attach file" title="Attachments are not available yet"><Plus size={16}/></button>
            {active && <button className="i-plus" onClick={() => void hardAbort()} aria-label="Hard stop" title="Hard stop (abort without checkpoint)"><Square size={10}/></button>}
          </div>
          {paused ? (
            <button className="i-send" disabled={resuming} onClick={() => void resume()} aria-label="Resume session" title="Resume from checkpoint">
              {resuming ? <Square size={13} fill="currentColor" className="spin-sm"/> : <RotateCcw size={16}/>}
            </button>
          ) : (
            <button className={`i-send${running ? ' stop' : ''}`} disabled={(!active && (!idle || !text.trim())) || pausing}
              onClick={() => active ? void requestPause() : void submit()} aria-label={active ? (pausing ? 'Pausing…' : 'Pause session') : 'Send'}>
              {active ? (pausing ? <Square size={13} fill="currentColor" className="spin-sm"/> : <Square size={13} fill="currentColor"/>) : <ArrowUp size={17}/>}
            </button>
          )}
        </div>
      </div>
      {!disabled && <p className="i-hint">Consensus Arena may make mistakes. Always review outputs. {paused && selectedSessionId ? 'Paused checkpoint saved — resume continues from next step.' : ''}</p>}
    </div>
  )
}
