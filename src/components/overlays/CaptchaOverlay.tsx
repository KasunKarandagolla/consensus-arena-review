import { ShieldAlert } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { displayName } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'

export default function CaptchaOverlay(){const {captchaPending,setCaptchaPending,addToast}=useAppStore();if(!captchaPending)return null;const id=captchaPending.agent_id
  async function resume(){try{await invoke('captcha_resolved',{agent_id:id});setCaptchaPending(null);addToast('Session resumed')}catch(e){console.error(e);addToast('Could not resume session')}}
  return <div className="ov captcha"><div className="ov-card"><div className="ov-icon"><ShieldAlert size={22}/></div><h3>Verification required</h3><p><strong>{displayName(id)}</strong> needs verification. Complete any login or security checks in the {displayName(id)} window, then click Resume to continue.</p><div className="ov-actions"><button className="ov-p" onClick={()=>void resume()}>Resume session</button></div></div></div>}
