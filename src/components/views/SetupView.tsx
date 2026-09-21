import { useEffect, useState } from 'react'
import { ChevronDown, ChevronUp, Cpu, Network, Plus, Settings2, Zap } from 'lucide-react'
import { open as openDirectory } from '@tauri-apps/plugin-dialog'
import { buildCommandErrorMessage, safeInvoke as invoke } from '@/lib/tauri'
import { loadParticipants } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'
import InputBar from '@/components/shared/InputBar'
import Topbar from '@/components/layout/Topbar'

type SessionType = 'architecture'|'mvp'|'api'|'security'|'custom'
interface BrainConfig { api_key:string;api_key_configured?:boolean;base_url:string;model:string;system_prompt:string }
interface ModelHealth { agent_id:string;is_available:boolean;error_count:number;last_error:string|null }
interface DshPrerequisite { runtime?:string; available?:boolean;compatible:boolean;executable:string|null;version:string|null;message:string }
interface CredentialStorageStatus { available:boolean; migration_pending:boolean; message:string }
const types:[SessionType,string][]=[['architecture','Architecture'],['mvp','MVP'],['api','API design'],['security','Security'],['custom','Custom']]
function deliveryStartMessage(details:string){
  const lower=details.toLowerCase()
  if(lower.includes('clean base working tree'))return 'This project has uncommitted changes. Commit or move them before starting Build.'
  if(lower.includes('project folder must be a git repository')||lower.includes('repository root'))return 'Choose the root folder of a Git project.'
  if(lower.includes('run git')&&(lower.includes('not found')||lower.includes('no such file')||lower.includes('cannot find')))return 'Git is not available on this device. Install Git, then try again.'
  if(lower.includes('credential store')||lower.includes('keyring')||lower.includes('credential storage'))return 'Your saved AI key is unavailable. Unlock the system credential store, then try again.'
  if(lower.includes('agent brain'))return 'Complete the primary AI service setup before starting Build.'
  return 'Build could not start. Check the project folder and setup, then try again.'
}

export default function SetupView(){
  const {setupBrief,setSetupBrief,setSessionStatus,setIsDraftSession,setSessionAgentIds,addToast,participants,setSettingsOpen, hackathonConfig, setHackathonOpen, setHackathonConfig, setActiveMode}=useAppStore()
  const [mode,setMode]=useState<'consult'|'delivery'>('consult')
  const [projectPath,setProjectPath]=useState('')
  const [sessionType,setType]=useState<SessionType>('architecture')
  const [selected,setSelected]=useState<Set<string>>(()=>new Set(['chatgpt','claude','deepseek']))
  const [leader,setLeader]=useState<string>('claude')
  const [brain,setBrain]=useState<BrainConfig>({api_key:'',base_url:'',model:'',system_prompt:''})
  const [health,setHealth]=useState<Record<string,ModelHealth>>({})
  const [dshPrerequisite,setDshPrerequisite]=useState<DshPrerequisite|null>(null)
  const [credentialStorageStatus,setCredentialStorageStatus]=useState<CredentialStorageStatus|null>(null)
  const [open,setOpen]=useState(false),[loading,setLoading]=useState(false),[error,setError]=useState('')
  const [errorDetails,setErrorDetails]=useState('')
  const [hackathonEnabled, setHackathonEnabled] = useState(false)

  useEffect(()=>{void Promise.allSettled([invoke<string>('get_agent_brain_config'),invoke<string>('get_agent_health'),loadParticipants(), invoke<string>('get_hackathon_config'),invoke<string>('get_credential_storage_status')]).then(results=>{
    if(results[0].status==='fulfilled')try{setBrain(JSON.parse(results[0].value) as BrainConfig)}catch(e){console.error(e)}
    if(results[1].status==='fulfilled')try{setHealth(JSON.parse(results[1].value) as Record<string,ModelHealth>)}catch(e){console.error(e)}
    if(results[3].status==='fulfilled')try{
      const cfg = JSON.parse(results[3].value) as import('@/stores/useAppStore').HackathonConfigSafe
      setHackathonConfig(cfg)
      setHackathonEnabled(Boolean(cfg?.enabled))
    }catch{}
    if(results[4].status==='fulfilled')try{setCredentialStorageStatus(JSON.parse(results[4].value) as CredentialStorageStatus)}catch(e){console.error(e)}
  })},[setHackathonConfig])
  useEffect(()=>{if(!selected.has(leader)){const first=participants.map(p=>p.agent_id).find(id=>selected.has(id));if(first)setLeader(first)}},[leader,selected,participants])
  useEffect(()=>{
    if(mode!=='delivery'){setDshPrerequisite(null);return}
    let disposed=false
    setDshPrerequisite(null)
    invoke<string>('get_dsh_prerequisite').then(raw=>{
      if(disposed)return
      try{setDshPrerequisite(JSON.parse(raw) as DshPrerequisite)}catch{setDshPrerequisite({available:false,compatible:false,executable:null,version:null,message:'Could not inspect the external DSH build worker.'})}
    }).catch(()=>{if(!disposed)setDshPrerequisite({available:false,compatible:false,executable:null,version:null,message:'Could not inspect the external DSH build worker.'})})
    return()=>{disposed=true}
  },[mode])

  function toggle(id:string){setSelected(current=>{const next=new Set(current);if(next.has(id)){if(next.size===2){setError('Select at least 2 participants.');return current}next.delete(id)}else next.add(id);setError('');return next})}
  const hasBrainKey=Boolean(brain.api_key.trim()||brain.api_key_configured)
  const brainReady=Boolean(hasBrainKey&&brain.base_url.trim()&&brain.model.trim()&&brain.system_prompt.trim())
  const openCodeReady=dshPrerequisite?.runtime==='opencode'
  const deliveryBrainReady=openCodeReady||Boolean(hasBrainKey&&brain.base_url.trim()&&brain.model.trim())
  const secureStorageReady=Boolean(credentialStorageStatus?.available&&!credentialStorageStatus.migration_pending)
  const deliveryStorageReady=secureStorageReady
  const canStart=mode==='delivery' ? Boolean(setupBrief.trim()&&projectPath.trim()&&dshPrerequisite?.compatible&&deliveryBrainReady&&deliveryStorageReady&&!loading) : Boolean(setupBrief.trim()&&selected.size>=2&&selected.has(leader)&&brainReady&&secureStorageReady&&!loading)
  useEffect(()=>{if(mode==='delivery'&&!deliveryBrainReady&&!openCodeReady)setOpen(true)},[mode,deliveryBrainReady,openCodeReady])
  async function chooseDirectory(){try{const chosen=await openDirectory({directory:true,multiple:false,title:'Choose a Git project'});if(typeof chosen==='string')setProjectPath(chosen)}catch(e){console.error(e);addToast('Could not open the folder picker')}}
  async function start(){
    if(!canStart){const storageBlocked=!secureStorageReady;const missingDeliveryBrain=mode==='delivery'&&!deliveryBrainReady;setError(storageBlocked?'Unlock secure credential storage and finish moving saved AI keys before starting a session.':missingDeliveryBrain?'Configure the primary Agent Brain API key, base URL, and model before Build mode.':mode==='delivery'?'Choose a project folder and describe the desired outcome.':'Complete the brief and agent brain configuration, then select at least 2 participants.');if(mode==='consult'||missingDeliveryBrain||storageBlocked)setOpen(true);return}
    setLoading(true);setError('');setErrorDetails('')
    let commandName='save_agent_brain_config'
    try{
      if(!openCodeReady){
        await invoke('save_agent_brain_config',{api_key:brain.api_key,base_url:brain.base_url,model:brain.model,system_prompt:brain.system_prompt})
        setBrain((current)=>({...current,api_key:'',api_key_configured:current.api_key_configured||Boolean(brain.api_key.trim())}))
        try {
          const rawStatus = await invoke<string>('get_credential_storage_status')
          setCredentialStorageStatus(JSON.parse(rawStatus) as CredentialStorageStatus)
        } catch (statusError) {
          console.error(statusError)
        }
      }
      if(mode==='delivery'){
        commandName='start_product_project'
        const rawRun=await invoke<string>('start_product_project',{founder_idea:setupBrief.trim(),repo_path:projectPath.trim()})
        const run=JSON.parse(rawRun) as {run_id:string}
        useAppStore.getState().setDeliveryState({session_id:run.run_id,phase:'preparing',attempt:0,objective:setupBrief.trim()})
        setActiveMode('delivery');setSessionStatus('running')
      }else{
        const ids=participants.map(p=>p.agent_id).filter(id=>selected.has(id))
        const setupOrder=[leader,...ids.filter(id=>id!==leader)]
        commandName='start_session'
        await invoke('start_session',{project_brief:setupBrief.trim(),session_type:sessionType,agent_ids:ids,leader_agent_id:leader})
        setSessionAgentIds(setupOrder);setIsDraftSession(false);setSessionStatus('setup');setActiveMode('consult')
      }
    }catch(e){const message=buildCommandErrorMessage(commandName,e);console.error(message);if(mode==='delivery'){setError(deliveryStartMessage(message));setErrorDetails(message);addToast(deliveryStartMessage(message),7000)}else{setError(message);addToast(message,7000)}}finally{setLoading(false)}
  }

  return <section className="view"><Topbar title="New session"/><div className="scroll pt" style={{display:'flex',flexDirection:'column',alignItems:'center'}}><div className="fw">
    <div className="fh">New session</div><div className="fh-sub">Configure your expert panel — every detail sharpens the output.</div>
    {error&&<div className="form-error">{error}{errorDetails&&<details style={{marginTop:8}}><summary style={{cursor:'pointer'}}>Setup details</summary><div style={{marginTop:6,overflowWrap:'anywhere'}}>{errorDetails}</div></details>}</div>}
    {credentialStorageStatus&&(!credentialStorageStatus.available||credentialStorageStatus.migration_pending)&&<div className="form-error" role="status">{credentialStorageStatus.message} <button className="sv-btn" style={{marginTop:8}} onClick={()=>setSettingsOpen(true)}>Open secure settings</button></div>}
    <div className="fg"><label className="fl">Mode</label><div className="seg"><button className={`sgo${mode==='consult'?' on':''}`} disabled={loading} onClick={()=>setMode('consult')}>Consult</button><button className={`sgo${mode==='delivery'?' on':''}`} disabled={loading} onClick={()=>setMode('delivery')}>Build</button></div></div>
    <div className="fg"><label className="fl">{mode==='delivery'?'Desired outcome':'Project brief'}</label><textarea className="fi" style={{minHeight:104}} value={setupBrief} disabled={loading} onChange={e=>setSetupBrief(e.target.value)} placeholder={mode==='delivery'?'Describe the change you want Arena to implement…':'Describe what you want to build. The more detail, the sharper the output.'}/></div>
    {mode==='delivery'&&<div className="fg"><label className="fl">Project folder</label><div style={{display:'flex',gap:8}}><input className="fi" value={projectPath} readOnly placeholder="Choose a clean Git repository"/><button className="sv-btn" disabled={loading} onClick={()=>void chooseDirectory()}>Choose folder</button></div></div>}
    {mode==='delivery'&&<div className="brain-note" role="status" style={{marginTop:4}}><Network size={15}/><div>{dshPrerequisite===null?'Checking Build setup…':dshPrerequisite.compatible?(openCodeReady?'OpenCode candidate runtime is ready.':'Build setup is ready.'):'Build cannot start yet because its required software could not be verified.'}{dshPrerequisite&&!dshPrerequisite.compatible&&<details style={{marginTop:5}}><summary style={{cursor:'pointer'}}>Setup details</summary><div style={{marginTop:5,overflowWrap:'anywhere'}}>{dshPrerequisite.message}</div></details>}</div></div>}
    {mode==='consult'&&<div className="fg"><label className="fl">Session type</label><div className="seg">{types.map(([value,label])=><button className={`sgo${sessionType===value?' on':''}`} key={value} disabled={loading} onClick={()=>setType(value)}>{label}</button>)}</div></div>}
    {mode==='consult'&&<div className="fg"><label className="fl">Participants <span className="fl-s">— pick 2 or more</span></label><div className="pcards">{participants.map(p=>{const on=selected.has(p.agent_id),available=health[p.agent_id]?.is_available;return <button className={`pc${on?' on':''}`} key={p.agent_id} disabled={loading} onClick={()=>toggle(p.agent_id)}><span className={`pcd${p.is_custom?' custom':available?'':' off'}`}/><span>{p.display_name}</span><span className={`tgl${on?' on':''}`}/></button>})}
      <button className="pc pc-add" disabled={loading} onClick={()=>setSettingsOpen(true)} title="Add a custom AI chat service by URL"><Plus size={15}/><span>Add custom AI</span></button>
    </div></div>}
    {mode==='consult'&&<div className="fg"><label className="fl">Leader model</label><select className="fi" disabled={loading} value={leader} onChange={e=>setLeader(e.target.value)}>{participants.filter(p=>selected.has(p.agent_id)).map(p=><option value={p.agent_id} key={p.agent_id}>{p.display_name}</option>)}</select></div>}
     {mode==='delivery'&&!deliveryBrainReady&&!openCodeReady&&<div className="brain-note" role="status"><Network size={15}/><span>Build mode needs the primary Agent Brain API key, base URL, and model. Configure them below to continue.</span></div>}
     {!openCodeReady&&<div className="fg"><div className="coll-h" onClick={()=>setOpen(v=>!v)} style={{borderRadius:open?'10px 10px 0 0':10}}><span><Cpu size={15}/>Agent brain</span>{open?<ChevronUp size={14}/>:<ChevronDown size={14}/>}</div>{open&&<div className="coll-body">
      <input className="fi" placeholder="API base URL" disabled={loading} value={brain.base_url} onChange={e=>setBrain({...brain,base_url:e.target.value})}/><input className="fi" type="password" placeholder={brain.api_key_configured?'Saved securely; leave blank to keep this key':'API key'} disabled={loading} value={brain.api_key} onChange={e=>setBrain({...brain,api_key:e.target.value})}/><input className="fi" placeholder="Model name" disabled={loading} value={brain.model} onChange={e=>setBrain({...brain,model:e.target.value})}/><textarea className="fi" style={{minHeight:72}} placeholder="System prompt..." disabled={loading} value={brain.system_prompt} onChange={e=>setBrain({...brain,system_prompt:e.target.value})}/>
      <div className="brain-note"><Network size={15}/><span>Optional fallback and secondary brains are available in Settings for reliability: fallback retries once; secondary takes over after repeated primary failures.</span></div>
    </div>}</div>}
    {mode==='consult'&&<div className="fg" style={{ border: '1px solid var(--border)', borderRadius: 12, padding: 14, background: 'var(--surface)' }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: hackathonEnabled ? 12 : 0 }}>
        <label className="fl" style={{ margin: 0, display: 'flex', alignItems: 'center', gap: 8 }}>
          <Zap size={14} style={{ color: 'var(--accent)' }} /> Hackathon Mode
          <span className="fl-s">— wide idea burst via API models</span>
        </label>
        <button
          role="switch"
          aria-checked={hackathonEnabled}
          aria-label="Hackathon mode"
          disabled={loading}
          className={`tgl${hackathonEnabled ? ' on' : ''}`}
          onClick={async () => {
            const next = !hackathonEnabled
            setHackathonEnabled(next)
            // Persist enabled flag — reuse save_hackathon_config with current groups/models
            try {
              const cfg = hackathonConfig ?? { groups: [], models: [], max_questions_per_teammate: 3, enabled: false }
              const toSave = { ...cfg, enabled: next, max_questions_per_teammate: cfg.max_questions_per_teammate ?? 3, groups: cfg.groups ?? [], models: (cfg.models as Array<{id:string;model_name:string;base_url:string;group_id:string;api_key?:string}>).map(m=>({ ...m, api_key: (m as {api_key?:string}).api_key ?? '' })) }
              await invoke('save_hackathon_config', { config: toSave })
              const raw = await invoke<string>('get_hackathon_config')
              setHackathonConfig(JSON.parse(raw) as import('@/stores/useAppStore').HackathonConfigSafe)
              if (next) setHackathonOpen(true)
            } catch (e) {
              console.error('[hackathon] toggle save failed', e)
              setHackathonEnabled(!next)
            }
          }}
        />
      </div>
      {hackathonEnabled && (
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          <span style={{ fontSize: 12.5, color: 'var(--t2)' }}>
            {hackathonConfig?.groups?.length ? `${hackathonConfig.groups.filter(g=>g.selected).length} team(s) selected · ${hackathonConfig.models.length} models` : 'No teams configured'}
          </span>
          <button className="cr-btn" disabled={loading} onClick={() => setHackathonOpen(true)} style={{ marginLeft: 'auto' }}>
            <Settings2 size={12} /> Configure Hackathon
          </button>
        </div>
      )}
      {!hackathonEnabled && (
        <p style={{ fontSize: 12, color: 'var(--t3)', marginTop: 8, lineHeight: 1.5 }}>Enable to run parallel API-model teams for divergent ideas. Output feeds the main leader as raw material.</p>
      )}
    </div>}
    <button className="btn-p" disabled={!canStart} onClick={()=>void start()}>{loading?'Starting…':mode==='delivery'?'Start build':'Start session'}</button>
  </div></div><InputBar/></section>
}
