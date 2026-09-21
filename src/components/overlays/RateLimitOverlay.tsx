import { Clock } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { displayName } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'

export default function RateLimitOverlay(){const {rateLimitPending,setRateLimitPending,addToast}=useAppStore();if(!rateLimitPending)return null;const {agent_id,estimated_reset_mins}=rateLimitPending
  async function decideContinue(){try{await invoke('rate_limit_decision',{agent_id,decision:'continue'});setRateLimitPending(null);addToast(`Continuing without ${displayName(agent_id)} — leader will avoid this participant`)}catch(e){console.error(e);addToast('Decision failed')}}
  async function decidePause(){try{await invoke('rate_limit_decision',{agent_id,decision:'wait'}); // mark unavailable
    try{ await invoke('request_pause',{}) }catch{}
    setRateLimitPending(null);addToast('Meeting paused — resume when ready')
  }catch(e){console.error(e);addToast('Decision failed')}}
  return <div className="ov"><div className="ov-card"><div className="ov-icon"><Clock size={22}/></div><h3>Rate limit reached</h3><p><strong>{displayName(agent_id)}</strong> is rate-limited (429). Estimated reset in <strong>{estimated_reset_mins} minutes</strong>.</p><p style={{fontSize:12,color:'var(--t3)',marginTop:4}}>Choose how to proceed. The current step will finish, then the chosen action applies — no data is lost.</p><div className="ov-actions"><button className="ov-p" onClick={()=>void decideContinue()}>Continue with existing members</button><button className="ov-g" onClick={()=>void decidePause()}>Temporary stop the meeting</button></div></div></div>}
