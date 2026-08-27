import { useEffect, useState } from 'react'
import { ChevronDown, ChevronUp, Cpu, Network, Plus } from 'lucide-react'
import { buildCommandErrorMessage, safeInvoke as invoke } from '@/lib/tauri'
import { loadParticipants } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'
import InputBar from '@/components/shared/InputBar'
import Topbar from '@/components/layout/Topbar'

type SessionType = 'architecture'|'mvp'|'api'|'security'|'custom'
interface BrainConfig { api_key:string;base_url:string;model:string;system_prompt:string }
interface ModelHealth { agent_id:string;is_available:boolean;error_count:number;last_error:string|null }
const types:[SessionType,string][]=[['architecture','Architecture'],['mvp','MVP'],['api','API design'],['security','Security'],['custom','Custom']]

export default function SetupView(){
  const {setupBrief,setSetupBrief,setSessionStatus,setSessionAgentIds,addToast,participants,setSettingsOpen}=useAppStore()
  const [sessionType,setType]=useState<SessionType>('architecture')
  const [selected,setSelected]=useState<Set<string>>(()=>new Set(['chatgpt','claude','deepseek']))
  const [leader,setLeader]=useState<string>('claude')
  const [brain,setBrain]=useState<BrainConfig>({api_key:'',base_url:'',model:'',system_prompt:''})
  const [health,setHealth]=useState<Record<string,ModelHealth>>({})
  const [open,setOpen]=useState(false),[loading,setLoading]=useState(false),[error,setError]=useState('')

  useEffect(()=>{void Promise.allSettled([invoke<string>('get_agent_brain_config'),invoke<string>('get_agent_health'),loadParticipants()]).then(results=>{
    if(results[0].status==='fulfilled')try{setBrain(JSON.parse(results[0].value) as BrainConfig)}catch(e){console.error(e)}
    if(results[1].status==='fulfilled')try{setHealth(JSON.parse(results[1].value) as Record<string,ModelHealth>)}catch(e){console.error(e)}
  })},[])
  useEffect(()=>{if(!selected.has(leader)){const first=participants.map(p=>p.agent_id).find(id=>selected.has(id));if(first)setLeader(first)}},[leader,selected,participants])

  function toggle(id:string){setSelected(current=>{const next=new Set(current);if(next.has(id)){if(next.size===2){setError('Select at least 2 participants.');return current}next.delete(id)}else next.add(id);setError('');return next})}
  const brainReady=Boolean(brain.api_key.trim()&&brain.base_url.trim()&&brain.model.trim()&&brain.system_prompt.trim())
  const canStart=Boolean(setupBrief.trim()&&selected.size>=2&&selected.has(leader)&&brainReady&&!loading)
  async function start(){if(!canStart){setError('Complete the brief and agent brain configuration, then select at least 2 participants.');setOpen(true);return}setLoading(true);setError('')
    try{await invoke('save_agent_brain_config',{api_key:brain.api_key,base_url:brain.base_url,model:brain.model,system_prompt:brain.system_prompt});const ids=participants.map(p=>p.agent_id).filter(id=>selected.has(id));const setupOrder=[leader,...ids.filter(id=>id!==leader)];await invoke('start_session',{project_brief:setupBrief.trim(),session_type:sessionType,agent_ids:ids,leader_agent_id:leader});setSessionAgentIds(setupOrder);setSessionStatus('setup')}
    catch(e){const message=buildCommandErrorMessage('start_session',e);console.error(message);setError(message);addToast(message,7000)}finally{setLoading(false)}}

  return <section className="view"><Topbar title="New session"/><div className="scroll pt" style={{display:'flex',flexDirection:'column',alignItems:'center'}}><div className="fw">
    <div className="fh">New session</div><div className="fh-sub">Configure your expert panel — every detail sharpens the output.</div>
    {error&&<div className="form-error">{error}</div>}
    <div className="fg"><label className="fl">Project brief</label><textarea className="fi" style={{minHeight:104}} value={setupBrief} onChange={e=>setSetupBrief(e.target.value)} placeholder="Describe what you want to build. The more detail, the sharper the output."/></div>
    <div className="fg"><label className="fl">Session type</label><div className="seg">{types.map(([value,label])=><button className={`sgo${sessionType===value?' on':''}`} key={value} onClick={()=>setType(value)}>{label}</button>)}</div></div>
    <div className="fg"><label className="fl">Participants <span className="fl-s">— pick 2 or more</span></label><div className="pcards">{participants.map(p=>{const on=selected.has(p.agent_id),available=health[p.agent_id]?.is_available;return <button className={`pc${on?' on':''}`} key={p.agent_id} onClick={()=>toggle(p.agent_id)}><span className={`pcd${p.is_custom?' custom':available?'':' off'}`}/><span>{p.display_name}</span><span className={`tgl${on?' on':''}`}/></button>})}
      <button className="pc pc-add" onClick={()=>setSettingsOpen(true)} title="Add a custom AI chat service by URL"><Plus size={15}/><span>Add custom AI</span></button>
    </div></div>
    <div className="fg"><label className="fl">Leader model</label><select className="fi" value={leader} onChange={e=>setLeader(e.target.value)}>{participants.filter(p=>selected.has(p.agent_id)).map(p=><option value={p.agent_id} key={p.agent_id}>{p.display_name}</option>)}</select></div>
    <div className="fg"><div className="coll-h" onClick={()=>setOpen(v=>!v)} style={{borderRadius:open?'10px 10px 0 0':10}}><span><Cpu size={15}/>Agent brain</span>{open?<ChevronUp size={14}/>:<ChevronDown size={14}/>}</div>{open&&<div className="coll-body">
      <input className="fi" placeholder="API base URL" value={brain.base_url} onChange={e=>setBrain({...brain,base_url:e.target.value})}/><input className="fi" type="password" placeholder="API key" value={brain.api_key} onChange={e=>setBrain({...brain,api_key:e.target.value})}/><input className="fi" placeholder="Model name" value={brain.model} onChange={e=>setBrain({...brain,model:e.target.value})}/><textarea className="fi" style={{minHeight:72}} placeholder="System prompt..." value={brain.system_prompt} onChange={e=>setBrain({...brain,system_prompt:e.target.value})}/>
      <div className="brain-note"><Network size={15}/><span>Optional fallback and secondary brains are available in Settings for reliability: fallback retries once; secondary takes over after repeated primary failures.</span></div>
    </div>}</div>
    <button className="btn-p" disabled={!canStart} onClick={()=>void start()}>{loading?'Starting…':'Start session'}</button>
  </div></div><InputBar/></section>
}
