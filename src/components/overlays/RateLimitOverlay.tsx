import { Clock } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { displayName } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'

type Decision='wait'|'continue'|'lighter'|'skip'
export default function RateLimitOverlay(){const {rateLimitPending,setRateLimitPending,addToast}=useAppStore();if(!rateLimitPending)return null;const {agent_id,estimated_reset_mins}=rateLimitPending
  async function decide(decision:Decision){try{await invoke('rate_limit_decision',{agent_id,decision});setRateLimitPending(null);addToast(decision==='lighter'?'Routing to lighter model':decision==='wait'?`Waiting for ${displayName(agent_id)}`:'Session updated')}catch(e){console.error(e);addToast('Decision failed')}}
  return <div className="ov"><div className="ov-card"><div className="ov-icon"><Clock size={22}/></div><h3>Rate limit reached</h3><p><strong>{displayName(agent_id)}</strong> has hit its rate limit. Estimated reset in <strong>{estimated_reset_mins} minutes</strong>.</p><div className="ov-actions"><button className="ov-p" onClick={()=>void decide('wait')}>Wait for reset</button><button className="ov-g" onClick={()=>void decide('continue')}>Continue without</button><button className="ov-g" onClick={()=>void decide('lighter')}>Use lighter model</button><button className="ov-g" onClick={()=>void decide('skip')}>Skip this model</button></div></div></div>}
