import { useState } from 'react'
import { Bell, Check, Circle } from 'lucide-react'
import { displayName } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'
import { safeInvoke as invoke } from '@/lib/tauri'
import InputBar from '@/components/shared/InputBar'
import Topbar from '@/components/layout/Topbar'

export default function PrimingView(){
  const {sessionAgentIds,setupProgress,setupReadyAgentId,setupFailedAgentId,addToast}=useAppStore()
  const [confirming,setConfirming]=useState<string | null>(null)
  const agents=sessionAgentIds.length?sessionAgentIds:[]
  const completed=new Set(setupProgress),done=agents.filter(id=>completed.has(id)).length,total=agents.length,pct=total?Math.round(done/total*100):0
  return <section className="view"><Topbar title="Setting up"/><div className="scroll pt" style={{display:'flex',flexDirection:'column',alignItems:'center'}}><div style={{maxWidth:520,width:'100%'}}>
    <div className="prime-h">Preparing your panel</div><div className="prime-sub">Priming each model with its role. Press Send in each model window when prompted, then come back here.</div>
    <div className="prog-wrap"><div className="prog-lbl"><span>{done} of {total} primed</span><span className="pct">{pct}%</span></div><div className="prog-track"><div className="prog-fill" style={{width:`${pct}%`}}/></div></div>
    <div className="plist">{agents.map(id=>{const isDone=completed.has(id),isCurrent=setupReadyAgentId===id,isFailed=setupFailedAgentId===id,canConfirm=isCurrent||isFailed;async function confirm(){setConfirming(id);try{await invoke('confirm_setup_agent',{agent_id:id})}catch(e){console.error(e);addToast('Could not confirm this model yet')}finally{setConfirming(null)}}return <div className={`prow ${isDone?'done':isCurrent?'cur':'wait'}`} key={id}><div className="prow-ic">{isDone?<Check size={14}/>:isCurrent?<span className="spin"/>:<Circle size={14}/>}</div><div className="prow-name">{displayName(id)}</div><div className="prow-st">{isDone?'Primed':isFailed?<><button className="sv-btn" onClick={()=>void invoke('retry_setup_agent',{agent_id:id}).catch(()=>addToast('Could not retry setup'))}>Retry setup</button>{canConfirm&&<button className="sv-btn" disabled={confirming===id} onClick={()=>void confirm()}>{confirming===id?'Confirming…':'I sent it / model responded'}</button>}</>:isCurrent?<button className="sv-btn" disabled={confirming===id} onClick={()=>void confirm()}>{confirming===id?'Confirming…':'I sent it / model responded'}</button>:'Queued'}</div></div>})}</div>
    {(setupReadyAgentId||setupFailedAgentId)&&<p style={{margin:'10px 0 0',fontSize:12,color:'var(--t3)'}}>Use manual confirmation only after you can see the prompt was sent or the model replied.</p>}
    <div className="info-box"><div className="ic"><Bell size={16}/></div><div><h4>{setupFailedAgentId?`${displayName(setupFailedAgentId)} setup can be retried`:setupReadyAgentId?`${displayName(setupReadyAgentId)} window is open`:'Preparing model windows'}</h4><p>{setupFailedAgentId?'Complete login, loading, or security checks in the model window, then retry. The session and any verified prompts are preserved. ':setupReadyAgentId?`Switch to the ${displayName(setupReadyAgentId)} window, review the priming prompt, and press Send. We’ll pick up automatically. `:''}The app uses only two WebViews: one persistent leader window and one shared navigating window.</p></div></div>
  </div></div><InputBar/></section>
}
