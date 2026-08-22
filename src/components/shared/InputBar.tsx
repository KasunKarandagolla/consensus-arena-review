import { useEffect, useRef, useState } from 'react'
import { ArrowUp, Plus, Square } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { useAppStore } from '@/stores/useAppStore'

export default function InputBar() {
  const { sessionStatus, setupBrief, setSetupBrief, setSessionStatus, addToast } = useAppStore()
  const [activeText, setActiveText] = useState('')
  const ref = useRef<HTMLTextAreaElement>(null)
  const idle = sessionStatus === 'idle'
  const running = sessionStatus === 'running' || sessionStatus === 'paused'
  const disabled = !idle && !running
  const text = idle ? setupBrief : activeText

  useEffect(() => { if (ref.current) { ref.current.style.height = 'auto'; ref.current.style.height = `${Math.min(ref.current.scrollHeight, 180)}px` } }, [text])

  async function submit() {
    const value = text.trim()
    if (!value) return
    if (idle) { setSetupBrief(value); setSessionStatus('setup'); return }
    try { await invoke('user_input', { text: value }); setActiveText(''); addToast('Context sent') }
    catch (error) { console.error(error); addToast('Could not send context') }
  }

  async function stop() {
    try { await invoke('abort_session') } catch (error) { console.error(error); addToast('Could not stop session') }
  }

  return (
    <div className="izone">
      <div className={`ibox${disabled ? ' disabled' : ''}`}>
        <textarea ref={ref} rows={2} disabled={disabled} value={text}
          placeholder={idle ? 'What are we building? Be as specific as you like.' : running ? 'Steer the session or add context...' : 'Waiting for setup to complete...'}
          onChange={e => idle ? setSetupBrief(e.target.value) : setActiveText(e.target.value)}
          onKeyDown={e => { if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void submit() } }}/>
        <div className="irow">
          <div className="irow-left"><button className="i-plus" disabled aria-label="Attach file" title="Attachments are not available yet"><Plus size={16}/></button></div>
          <button className={`i-send${running ? ' stop' : ''}`} disabled={!running && (!idle || !text.trim())}
            onClick={() => running ? void stop() : void submit()} aria-label={running ? 'Stop session' : 'Send'}>
            {running ? <Square size={13} fill="currentColor"/> : <ArrowUp size={17}/>}
          </button>
        </div>
      </div>
      {!disabled && <p className="i-hint">Consensus Arena may make mistakes. Always review outputs.</p>}
    </div>
  )
}
