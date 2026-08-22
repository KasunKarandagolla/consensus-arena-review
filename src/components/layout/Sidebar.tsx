import { useCallback, useEffect, useRef, useState } from 'react'
import { Download, Info, MessageSquare, MoreHorizontal, Pencil, RotateCcw, Settings2, SquarePen, Trash2 } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { displayName } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'
import SettingsPanel from '@/panels/SettingsPanel'

interface Session{id:string;project_brief:string;session_type:string;created_at:number;status:string}
interface ModelHealth{agent_id:string;is_available:boolean;error_count:number;last_error:string|null}
interface Details{project_brief:string;status:string;turn_count:number;section_count:number;agent_ids:string[]}
interface Menu{sessionId:string;x:number;y:number}

export default function Sidebar(){
  const {selectedSessionId,recoveryState,settingsOpen,sidebarCollapsed,sessionStatus,setSessionStatus,setSelectedSessionId,setRecoveryState,clearSessionState,setSettingsOpen,addToast,participants}=useAppStore()
  const [sessions,setSessions]=useState<Session[]>([]),[health,setHealth]=useState<Record<string,ModelHealth>>({}),[menu,setMenu]=useState<Menu|null>(null),[rename,setRename]=useState<string|null>(null),[renameValue,setRenameValue]=useState('')
  const menuRef=useRef<HTMLDivElement>(null),renameRef=useRef<HTMLInputElement>(null)
  const loadSessions=useCallback(async()=>{try{setSessions(JSON.parse(await invoke<string>('get_session_list')) as Session[])}catch(e){console.error(e)}},[])
  const loadHealth=useCallback(async()=>{try{setHealth(JSON.parse(await invoke<string>('get_agent_health')) as Record<string,ModelHealth>)}catch(e){console.error(e)}},[])
  useEffect(()=>{void loadSessions();void loadHealth();const timer=setInterval(()=>void loadHealth(),15000);return()=>clearInterval(timer)},[loadHealth,loadSessions,sessionStatus])
  useEffect(()=>{if(!menu)return;const close=(e:MouseEvent)=>{if(!menuRef.current?.contains(e.target as Node))setMenu(null)};document.addEventListener('mousedown',close);return()=>document.removeEventListener('mousedown',close)},[menu])
  useEffect(()=>renameRef.current?.focus(),[rename])
  function newSession(){clearSessionState();setSelectedSessionId(null);setSessionStatus('setup')}
  async function recover(){if(!recoveryState)return;try{clearSessionState();await invoke('recover_session',{session_id:recoveryState.session_id});setSelectedSessionId(recoveryState.session_id);setSessionStatus('ended');setRecoveryState(null);addToast('Recovery loaded')}catch(e){console.error(e);addToast('Recovery failed')}}
  async function remove(id:string){setMenu(null);try{await invoke('delete_session',{session_id:id});if(selectedSessionId===id){setSelectedSessionId(null);setSessionStatus('idle')}await loadSessions();addToast('Session deleted')}catch(e){console.error(e);addToast(String(e))}}
  async function exportSession(id:string){setMenu(null);try{const path=await invoke<string>('export_blueprint',{format:'markdown',session_id:id});addToast(`Saved to ${path}`)}catch(e){console.error(e);addToast('Export failed')}}
  async function details(id:string){setMenu(null);try{const data=JSON.parse(await invoke<string>('get_session_details',{session_id:id})) as Details;      addToast(`${data.status} · ${data.turn_count} turns · ${data.section_count} sections · ${data.agent_ids.map((id) => displayName(id)).join(', ')||'No agents yet'}`,5000)}catch(e){console.error(e);addToast('Could not load details')}}
  async function commit(id:string){const title=renameValue.trim();setRename(null);if(!title)return;try{await invoke('rename_session',{session_id:id,title});await loadSessions();addToast('Session renamed')}catch(e){console.error(e);addToast('Rename failed')}}
  return <><aside className={`sidebar${sidebarCollapsed?' closed':''}`}><div className="sb-inner"><div className="sb-top"><div className="sb-logo-row"><span className="sb-logo">Consensus&nbsp;<em style={{fontStyle:'normal'}}>Arena</em></span></div><button className="sb-new" onClick={newSession}><SquarePen size={15}/>New session</button>{recoveryState&&<div className="recover-card"><RotateCcw size={15}/><span>Recover incomplete session</span><button onClick={()=>void recover()}>Recover</button></div>}</div>
    <div className="sb-lbl">Recent <span className="badge">{sessions.length}</span></div><div className="sb-scroll">{sessions.length===0?<div style={{padding:20,textAlign:'center',fontSize:12.5,color:'var(--t3)'}}>No sessions yet</div>:sessions.map(session=><div className={`si${selectedSessionId===session.id?' on':''}`} key={session.id} onClick={()=>setSelectedSessionId(session.id)}><MessageSquare size={14}/>{rename===session.id?<input ref={renameRef} className="rename-input" value={renameValue} onClick={e=>e.stopPropagation()} onChange={e=>setRenameValue(e.target.value)} onBlur={()=>void commit(session.id)} onKeyDown={e=>{if(e.key==='Enter')void commit(session.id);if(e.key==='Escape')setRename(null)}}/>:<span className="si-name">{session.project_brief}</span>}<button className="si-dots" aria-label="Session options" onClick={e=>{e.stopPropagation();setMenu({sessionId:session.id,x:Math.min(e.clientX,window.innerWidth-185),y:Math.min(e.clientY+8,window.innerHeight-180)})}}><MoreHorizontal size={14}/></button></div>)}</div>
    <div className="sb-foot"><div className="sb-models"><span className="sb-mlbl">Connected models</span><div className="mdots">{participants.map(p=><span className={`md${health[p.agent_id]?.is_available?' on':' off'}${p.is_custom?' custom':''}`} title={`${p.display_name}${health[p.agent_id]?.last_error?`: ${health[p.agent_id].last_error}`:''}`} key={p.agent_id}/>)}</div></div><div className="acct" onClick={()=>setSettingsOpen(true)}><div className="ava">CA</div><span className="acct-n">Account</span><Settings2 size={15}/></div></div>
  </div></aside>
  {menu&&<div className="ctx" ref={menuRef} style={{left:menu.x,top:menu.y}}><button className="ctx-item" onClick={()=>{const s=sessions.find(x=>x.id===menu.sessionId);if(s){setRename(menu.sessionId);setRenameValue(s.project_brief)}setMenu(null)}}><Pencil size={14}/>Rename session</button><button className="ctx-item" onClick={()=>void exportSession(menu.sessionId)}><Download size={14}/>Export blueprint</button><button className="ctx-item" onClick={()=>void details(menu.sessionId)}><Info size={14}/>Session details</button><div className="ctx-sep"/><button className="ctx-item danger" onClick={()=>void remove(menu.sessionId)}><Trash2 size={14}/>Delete</button></div>}
  <SettingsPanel open={settingsOpen} onClose={()=>setSettingsOpen(false)}/></>
}
