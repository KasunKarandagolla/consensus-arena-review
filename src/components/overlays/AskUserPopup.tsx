import { useCallback, useEffect, useRef, useState } from 'react'
import { MessageCircleQuestion, Send } from 'lucide-react'
import { safeInvoke as invoke } from '@/lib/tauri'
import { useAppStore } from '@/stores/useAppStore'

export default function AskUserPopup(){
  const {askUserPending,setAskUserPending}=useAppStore(),[custom,setCustom]=useState(''),answering=useRef(false),input=useRef<HTMLInputElement>(null)
  const answer=useCallback(async(value:string)=>{if(answering.current)return;answering.current=true;try{await invoke('provide_user_answer',{answer:value})}catch(e){console.error('provide_user_answer failed',e)}finally{setAskUserPending(null)}},[setAskUserPending])
  useEffect(()=>{const key=(e:KeyboardEvent)=>{if(e.key==='Escape')void answer('Cancelled')};document.addEventListener('keydown',key);return()=>document.removeEventListener('keydown',key)},[answer])
  useEffect(()=>{if(askUserPending?.allow_custom)setTimeout(()=>input.current?.focus(),80)},[askUserPending])
  if(!askUserPending)return null
  return <div className="ov ask" onClick={e=>{if(e.target===e.currentTarget)void answer('Cancelled')}}><div className="ov-card ask-card" onClick={e=>e.stopPropagation()}><div className="ov-icon"><MessageCircleQuestion size={22}/></div><h3>Agent needs input</h3><p>{askUserPending.question}</p><div className="ask-options">{askUserPending.options.map(option=><button className="ask-option" key={option} onClick={()=>void answer(option)}>{option}</button>)}</div>{askUserPending.allow_custom&&<div className="ask-input"><input ref={input} className="si2" placeholder="Custom answer..." value={custom} onChange={e=>setCustom(e.target.value)} onKeyDown={e=>{if(e.key==='Enter'&&custom.trim())void answer(custom.trim())}}/><button className="ov-p" disabled={!custom.trim()} onClick={()=>void answer(custom.trim())}><Send size={14}/></button></div>}<div className="ask-hint">Press Esc or click outside to cancel</div></div></div>
}
