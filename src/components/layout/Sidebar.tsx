import { useCallback, useEffect, useRef, useState } from 'react'
import { Download, Info, MessageSquare, MoreHorizontal, Pencil, RotateCcw, Settings2, SquarePen, Trash2, X, Check } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { displayName } from '@/lib/agents'
import { useAppStore } from '@/stores/useAppStore'
import SettingsPanel from '@/panels/SettingsPanel'

interface Session{id:string;project_brief:string;session_type:string;created_at:number;status:string}
interface ModelHealth{agent_id:string;is_available:boolean;error_count:number;last_error:string|null}
interface Details{project_brief:string;status:string;turn_count:number;section_count:number;agent_ids:string[]}
interface Menu{sessionId:string;x:number;y:number}

export default function Sidebar(){
  const {selectedSessionId,recoveryState,settingsOpen,sidebarCollapsed,sessionStatus,setSessionStatus,setIsDraftSession,setSelectedSessionId,setRecoveryState,clearSessionState,setSettingsOpen,addToast,participants,clearBlueprintSections,appendBlueprintSection,setSetupBrief}=useAppStore()
  const [sessions,setSessions]=useState<Session[]>([]),[health,setHealth]=useState<Record<string,ModelHealth>>({}),[menu,setMenu]=useState<Menu|null>(null),[rename,setRename]=useState<string|null>(null),[renameValue,setRenameValue]=useState('')
  const menuRef=useRef<HTMLDivElement>(null),renameRef=useRef<HTMLInputElement>(null)
  const loadSeqRef=useRef(0)
  const [selectedIds,setSelectedIds]=useState<Set<string>>(new Set())
  const [selectionMode,setSelectionMode]=useState(false)
  const [headingMenuOpen,setHeadingMenuOpen]=useState(false)
  const headingMenuRef=useRef<HTMLDivElement>(null)
  const headingDotsRef=useRef<HTMLButtonElement>(null)
  const loadSessions=useCallback(async()=>{try{setSessions(JSON.parse(await invoke<string>('get_session_list')) as Session[])}catch(e){console.error(e)}},[])
  const loadHealth=useCallback(async()=>{try{setHealth(JSON.parse(await invoke<string>('get_agent_health')) as Record<string,ModelHealth>)}catch(e){console.error(e)}},[])
  useEffect(()=>{void loadSessions();void loadHealth();const timer=setInterval(()=>void loadHealth(),15000);return()=>clearInterval(timer)},[loadHealth,loadSessions,sessionStatus])
  useEffect(()=>{if(!menu)return;const close=(e:MouseEvent)=>{if(!menuRef.current?.contains(e.target as Node))setMenu(null)};document.addEventListener('mousedown',close);return()=>document.removeEventListener('mousedown',close)},[menu])
  useEffect(()=>{if(!headingMenuOpen)return;const close=(e:MouseEvent)=>{if(headingMenuRef.current?.contains(e.target as Node) || headingDotsRef.current?.contains(e.target as Node))return;setHeadingMenuOpen(false)};document.addEventListener('mousedown',close);return()=>document.removeEventListener('mousedown',close)},[headingMenuOpen])
  useEffect(()=>renameRef.current?.focus(),[rename])
  function newSession(){loadSeqRef.current+=1;clearSessionState();setSelectedSessionId(null);setSessionStatus('setup');setIsDraftSession(true)}
  async function recover(){if(!recoveryState)return;try{clearSessionState();await invoke('recover_session',{session_id:recoveryState.session_id});setSelectedSessionId(recoveryState.session_id);setSessionStatus('ended');setRecoveryState(null);addToast('Recovery loaded')}catch(e){console.error(e);addToast('Recovery failed')}}
  async function remove(id:string){setMenu(null);try{await invoke('delete_session',{session_id:id});if(selectedSessionId===id){setSelectedSessionId(null);setSessionStatus('idle')}await loadSessions();addToast('Session deleted')}catch(e){console.error(e);addToast(String(e))}}
  async function exportSession(id:string){setMenu(null);try{const path=await invoke<string>('export_blueprint',{format:'markdown',session_id:id});addToast(`Saved to ${path}`)}catch(e){console.error(e);addToast('Export failed')}}
  async function details(id:string){setMenu(null);try{const data=JSON.parse(await invoke<string>('get_session_details',{session_id:id})) as Details;      addToast(`${data.status} · ${data.turn_count} turns · ${data.section_count} sections · ${data.agent_ids.map((id) => displayName(id)).join(', ')||'No agents yet'}`,5000)}catch(e){console.error(e);addToast('Could not load details')}}
  async function commit(id:string){const title=renameValue.trim();setRename(null);if(!title)return;try{await invoke('rename_session',{session_id:id,title});await loadSessions();addToast('Session renamed')}catch(e){console.error(e);addToast('Rename failed')}}
  function toggleSelectOne(id:string){
    setSelectedIds(prev=>{
      const next=new Set(prev)
      if(next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }
  function selectAll(){ setSelectedIds(new Set(sessions.map(s=>s.id))) }
  function deselectAll(){ setSelectedIds(new Set()) }
  async function deleteSelected(){
    if(selectedIds.size===0) { addToast('No chats selected'); return }
    const activeId = selectedSessionId
    const toDelete = Array.from(selectedIds).filter(id => id !== activeId)
    const skippedActive = selectedIds.size - toDelete.length
    if(toDelete.length===0){
      addToast(skippedActive ? 'Cannot delete the active session — stop it first' : 'No chats selected')
      return
    }
    const confirmMsg = skippedActive
      ? `Delete ${toDelete.length} selected session(s)? (${skippedActive} active session will be skipped)`
      : `Delete ${toDelete.length} selected session(s)?`
    if(!confirm(confirmMsg)) return
    let deleted=0, failed=0
    for(const id of toDelete){
      try{ await invoke('delete_session',{session_id:id}); deleted++ }catch(e){ console.error(e); failed++ }
    }
    const nextSelected = new Set<string>()
    // Keep active if it was selected (skipped)
    if (activeId && selectedIds.has(activeId)) {
      // active stays selected? Actually we skipped it, so keep selection mode but with only active? Better clear.
      // Spec: clear selection state; exit selection mode if no selected remain
      // Since we skipped active, we should clear anyway
    }
    setSelectedIds(nextSelected)
    setSelectionMode(false)
    setHeadingMenuOpen(false)
    await loadSessions()
    if(failed) addToast(`Deleted ${deleted}, ${failed} failed`)
    else addToast(`${deleted} session(s) deleted`)
  }
  const handleSelectSession=useCallback(async(id:string)=>{
    const seq=++loadSeqRef.current
    setSelectedSessionId(id)
    clearBlueprintSections()
    try{
      const [detailsRaw, blueprintRaw]=await Promise.all([
        invoke<string>('get_session_details',{session_id:id}),
        invoke<string>('get_blueprint_sections',{session_id:id}),
      ])
      if(loadSeqRef.current!==seq) return
      const detailsData=JSON.parse(detailsRaw) as Details & { id:string }
      const sections=JSON.parse(blueprintRaw) as Array<{id:string;title:string;content:string;status:string}>
      if(detailsData.project_brief) setSetupBrief(detailsData.project_brief)
      clearBlueprintSections()
      for(const s of sections){
        if(loadSeqRef.current!==seq) return
        appendBlueprintSection({id:s.id,title:s.title,content:s.content,status:(s.status as 'draft'|'agreed'|'negotiation'|'disputed')||'agreed'})
      }
      try{
        const cpRaw=await invoke<string>('get_session_checkpoint',{session_id:id})
        if(loadSeqRef.current!==seq) return
        const cp=cpRaw && cpRaw!=='null' ? JSON.parse(cpRaw) as {session_id:string;status:string} : null
        if(cp && cp.status==='paused'){
          setSessionStatus('paused')
          addToast('Paused session loaded — press Resume to continue')
        }else if(detailsData.status==='complete' || sections.length>0){
          setSessionStatus('complete')
        }else{
          setSessionStatus('ended')
        }
      }catch{
        if(loadSeqRef.current!==seq) return
        if(detailsData.status==='complete' || sections.length>0) setSessionStatus('complete')
        else setSessionStatus('ended')
      }
    }catch(e){
      if(loadSeqRef.current!==seq) return
      console.error('[sidebar] load session failed',e)
      addToast('Could not load session')
    }
  },[setSelectedSessionId,clearBlueprintSections,appendBlueprintSection,setSetupBrief,setSessionStatus,addToast])
  const selectionCount = selectedIds.size
  const allSelected = sessions.length>0 && selectionCount===sessions.length
  return <><aside className={`sidebar${sidebarCollapsed?' closed':''}`}><div className="sb-inner"><div className="sb-top"><div className="sb-logo-row"><span className="sb-logo">Consensus&nbsp;<em style={{fontStyle:'normal'}}>Arena</em></span></div><button className="sb-new" onClick={newSession}><SquarePen size={15}/>New session</button>{recoveryState&&<div className="recover-card"><RotateCcw size={15}/><span>Recover incomplete session</span><button onClick={()=>void recover()}>Recover</button></div>}</div>
    <div className="sb-lbl" style={{position:'relative', display:'flex', alignItems:'center', gap:6}}>
      {!selectionMode ? (
        <>
          <span>Recent</span> <span className="badge">{sessions.length}</span>
          <button
            ref={headingDotsRef}
            className="sb-heading-dots"
            aria-label="Recent chats options"
            aria-expanded={headingMenuOpen}
            aria-haspopup="menu"
            onClick={() => setHeadingMenuOpen(v=>!v)}
            style={{marginLeft:6, width:22, height:22, borderRadius:6, display:'flex', alignItems:'center', justifyContent:'center', color: headingMenuOpen ? 'var(--accent)' : 'var(--t3)', background: headingMenuOpen ? 'var(--accent-soft)' : 'transparent', border: headingMenuOpen ? '1px solid var(--accent-mid)' : '1px solid transparent', flexShrink:0}}
          >
            <MoreHorizontal size={13}/>
          </button>
          {headingMenuOpen && (
            <div ref={headingMenuRef} className="sb-heading-menu" role="menu" style={{position:'absolute', left: 90, top: 22, zIndex:20, minWidth:140, background:'var(--surface-elev)', border:'1px solid var(--border)', borderRadius:10, boxShadow:'var(--sh-xl)', padding:4}}>
              <button
                role="menuitem"
                onClick={() => { setSelectionMode(true); setHeadingMenuOpen(false); setSelectedIds(new Set()) }}
                style={{width:'100%', textAlign:'left', padding:'7px 10px', borderRadius:7, fontSize:12, fontWeight:600, color:'var(--t2)', display:'flex', alignItems:'center', gap:7}}
              >
                <Check size={12}/> Select chats
              </button>
            </div>
          )}
        </>
      ) : (
        <>
          <span>Recent</span> <span className="badge">{sessions.length}</span>
          <span style={{fontSize:11, fontWeight:600, color: selectionCount? 'var(--accent)' : 'var(--t3)', marginLeft:6}}>{selectionCount}/{sessions.length}</span>
          <div style={{marginLeft:'auto', display:'flex', alignItems:'center', gap:4}}>
            <button
              className="sb-bin"
              disabled={selectionCount===0}
              onClick={() => void deleteSelected()}
              title={selectionCount===0 ? 'No chats selected' : `Delete ${selectionCount} selected`}
              style={{width:22, height:22, borderRadius:6, display:'flex', alignItems:'center', justifyContent:'center', color: selectionCount===0 ? 'var(--t3)' : 'var(--red)', background: selectionCount===0 ? 'transparent' : 'var(--red-soft)', border: `1px solid ${selectionCount===0 ? 'var(--border)' : 'var(--red-mid, var(--red))'}`, opacity: selectionCount===0 ? .5 : 1, cursor: selectionCount===0 ? 'not-allowed' : 'pointer'}}
            >
              <Trash2 size={12}/>
            </button>
            <button
              onClick={() => { setSelectionMode(false); setSelectedIds(new Set()); setHeadingMenuOpen(false) }}
              title="Exit selection mode"
              style={{width:22, height:22, borderRadius:6, display:'flex', alignItems:'center', justifyContent:'center', color:'var(--t3)', background:'var(--surface)', border:'1px solid var(--border)', flexShrink:0}}
            >
              <X size={12}/>
            </button>
          </div>
          <div style={{display:'flex', gap:4, marginLeft:8, flexShrink:0}}>
            <button
              onClick={allSelected ? deselectAll : selectAll}
              style={{fontSize:11, fontWeight:600, color:'var(--accent)', background:'transparent', padding:'2px 6px', borderRadius:6, border:'1px solid var(--accent-mid)'}}
            >
              {allSelected ? 'Deselect all' : 'Select all'}
            </button>
          </div>
        </>
      )}
    </div>
    <div className="sb-scroll">{sessions.length===0?<div style={{padding:20,textAlign:'center',fontSize:12.5,color:'var(--t3)'}}>No sessions yet</div>:sessions.map(session=>{
      const isSelectedRow = selectionMode && selectedIds.has(session.id)
      const isActiveRow = selectedSessionId===session.id && !selectionMode
      return <div className={`si${isActiveRow?' on':''}${isSelectedRow?' sel':''}`} key={session.id} onClick={()=>{
        if(selectionMode){ toggleSelectOne(session.id)} else { void handleSelectSession(session.id)}
      }}>{selectionMode ? (
        <button
          className={`sb-check${isSelectedRow?' on':''}`}
          aria-label={isSelectedRow ? 'Deselect' : 'Select'}
          onClick={e=>{e.stopPropagation(); toggleSelectOne(session.id)}}
          style={{width:14, height:14, borderRadius:4, border: `1.5px solid ${isSelectedRow ? 'var(--accent)' : 'var(--border2)'}`, background: isSelectedRow ? 'var(--accent)' : 'var(--surface)', display:'flex', alignItems:'center', justifyContent:'center', flexShrink:0}}
        >
          {isSelectedRow && <Check size={8} color="#fff"/>}
        </button>
      ) : <MessageSquare size={14}/> }{rename===session.id?<input ref={renameRef} className="rename-input" value={renameValue} onClick={e=>e.stopPropagation()} onChange={e=>setRenameValue(e.target.value)} onBlur={()=>void commit(session.id)} onKeyDown={e=>{if(e.key==='Enter')void commit(session.id);if(e.key==='Escape')setRename(null)}}/>:<span className="si-name">{session.project_brief}</span>}{!selectionMode && <button className="si-dots" aria-label="Session options" onClick={e=>{e.stopPropagation();setHeadingMenuOpen(false);setMenu({sessionId:session.id,x:Math.min(e.clientX,window.innerWidth-185),y:Math.min(e.clientY+8,window.innerHeight-180)})}}><MoreHorizontal size={14}/></button>}</div>})}</div>
    <div className="sb-foot"><div className="sb-models"><span className="sb-mlbl">Connected models</span><div className="mdots">{participants.map(p=><span className={`md${health[p.agent_id]?.is_available?' on':' off'}${p.is_custom?' custom':''}`} title={`${p.display_name}${health[p.agent_id]?.last_error?`: ${health[p.agent_id].last_error}`:''}`} key={p.agent_id}/>)}</div></div><div className="acct" onClick={()=>setSettingsOpen(true)}><div className="ava">CA</div><span className="acct-n">Account</span><Settings2 size={15}/></div></div>
  </div></aside>
  {menu&&<div className="ctx" ref={menuRef} style={{left:menu.x,top:menu.y}}><button className="ctx-item" onClick={()=>{const s=sessions.find(x=>x.id===menu.sessionId);if(s){setRename(menu.sessionId);setRenameValue(s.project_brief)}setMenu(null)}}><Pencil size={14}/>Rename session</button><button className="ctx-item" onClick={()=>void exportSession(menu.sessionId)}><Download size={14}/>Export blueprint</button><button className="ctx-item" onClick={()=>void details(menu.sessionId)}><Info size={14}/>Session details</button><div className="ctx-sep"/><button className="ctx-item danger" onClick={()=>void remove(menu.sessionId)}><Trash2 size={14}/>Delete</button></div>}
  <SettingsPanel open={settingsOpen} onClose={()=>setSettingsOpen(false)}/></>
}
