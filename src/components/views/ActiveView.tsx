import { useEffect, useRef, useState } from 'react'
import ReactMarkdown from 'react-markdown'
import { Bot, Check, ChevronUp, Copy, Download } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { displayName } from '@/lib/agents'
import { useAppStore, type BlueprintSection } from '@/stores/useAppStore'
import InputBar from '@/components/shared/InputBar'
import Topbar from '@/components/layout/Topbar'

function SectionCard({section,index}:{section:BlueprintSection;index:number}){
  const [copied,setCopied]=useState(false)
  async function copy(){try{await navigator.clipboard.writeText(section.content);setCopied(true);setTimeout(()=>setCopied(false),1800)}catch(e){console.error(e)}}
  return <article className="bp" style={{animationDelay:`${index*.08}s`}}><div className="bp-head"><span className="bp-num">{String(index+1).padStart(2,'0')}</span><h3 className="bp-title">{section.title}</h3><span className="bp-meta">{section.status}</span><button className="bp-copy" onClick={()=>void copy()}>{copied?<Check size={12}/>:<Copy size={12}/>} {copied?'Copied':'Copy'}</button></div><div className="bp-markdown"><ReactMarkdown>{section.content}</ReactMarkdown></div></article>
}

export default function ActiveView(){
  const {blueprintSections,liveStatusText,liveStatusExpanded,currentAgentResponse,sessionStatus,sessionAgentIds,activeAgentId,activeTurnNumber,setupBrief,setLiveStatusExpanded,addToast}=useAppStore()
  const endRef=useRef<HTMLDivElement>(null)
  const [manualOpen,setManualOpen]=useState(false),[manualResponse,setManualResponse]=useState(''),[submitting,setSubmitting]=useState(false)
  useEffect(()=>endRef.current?.scrollIntoView({behavior:'smooth'}),[blueprintSections.length])
  const running=sessionStatus==='running'||sessionStatus==='paused',complete=sessionStatus==='complete',ended=sessionStatus==='ended'
  async function download(){try{const path=await invoke<string>('export_blueprint',{format:'markdown'});addToast(`Saved to ${path}`)}catch(e){console.error(e);addToast('Export failed')}}
  async function useManualResponse(){if(!activeAgentId||activeTurnNumber===null||!manualResponse.trim())return;setSubmitting(true);try{await invoke('provide_manual_model_response',{agent_id:activeAgentId,turn_number:activeTurnNumber,response:manualResponse.trim()});setManualOpen(false);setManualResponse('')}catch(e){console.error(e);addToast('Could not use this response for the current turn')}finally{setSubmitting(false)}}
  const title=setupBrief.trim()?setupBrief.trim().slice(0,48):'Blueprint'
  return <section className="view"><Topbar title={title} right={<div className="sess-badge"><span className="sess-badge-dot"/>{complete?'Session complete':ended?'Session ended':'Session active'}</div>}/>
    {blueprintSections.length>0&&<div style={{display:'flex',justifyContent:'flex-end',padding:'14px 16% 0'}}><button className="dl-btn" onClick={()=>void download()}><Download size={14}/>Download</button></div>}
    <div className="scroll blueprint-scroll">{blueprintSections.length?<>{blueprintSections.map((section,index)=><SectionCard section={section} index={index} key={section.id}/>) }<div ref={endRef}/></>:<div className="active-empty">{running&&<><span className="spin" style={{width:24,height:24}}/><span>Building your blueprint…</span></>}</div>}</div>
    {(liveStatusText||running)&&<><div className="stline" onClick={()=>setLiveStatusExpanded(!liveStatusExpanded)}><div className="models">{sessionAgentIds.map(id=><div className={`md-st${activeAgentId===id?' thinking':' idle'}`} key={id}><span className="dot"/>{displayName(id)}</div>)}</div><div className="s-txt">{liveStatusText||'Waiting for the panel to respond…'}</div><span className="s-chev"><ChevronUp size={14} style={{transform:liveStatusExpanded?'rotate(180deg)':'none',transition:'transform .3s'}}/></span></div>
      <div className={`sdrawer${liveStatusExpanded?' open':''}`}><div className="sdi"><div className="sdi-label"><Bot size={12}/>{activeAgentId?displayName(activeAgentId):'Agent'} — current response</div><div className="sdi-txt">{currentAgentResponse||'No model response has been received yet.'}</div></div></div></>}
    {running&&activeAgentId&&activeTurnNumber!==null&&<div style={{padding:'0 16% 10px'}}><button className="sv-btn" onClick={()=>setManualOpen(!manualOpen)}>I can see the model response</button>{manualOpen&&<div style={{marginTop:8,display:'grid',gap:8}}><label style={{fontSize:12,color:'var(--t3)'}}>If auto-submit failed, click Send in the model window first. Then paste the visible response here to continue.</label><textarea className="fi" rows={5} value={manualResponse} onChange={e=>setManualResponse(e.target.value)} placeholder="Paste only the model response shown in the model window."/><button className="sv-btn" disabled={submitting||!manualResponse.trim()} onClick={()=>void useManualResponse()}>{submitting?'Using response…':'Use this response'}</button></div>}</div>}
    <InputBar/>
  </section>
}
