import { ShieldAlert } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { displayName } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'

export default function CaptchaOverlay(){const {captchaPending,setCaptchaPending,addToast}=useAppStore();if(!captchaPending)return null;const id=captchaPending.agent_id
  async function resume(){try{await invoke('captcha_resolved',{agent_id:id});setCaptchaPending(null);addToast('Checking page readiness…')}catch(e){console.error(e);addToast('Could not check verification state')}}
  return <div className="ov captcha"><div className="ov-card"><div className="ov-icon"><ShieldAlert size={22}/></div><h3>Verification required</h3><p><strong>{displayName(id)}</strong> needs verification. Complete any login or security checks in the {displayName(id)} window, then ask Arena to check the page again. Arena continues only after the model page reports a ready composer.</p><div className="ov-actions"><button className="ov-p" onClick={()=>void resume()}>Check verification</button></div></div></div>}
