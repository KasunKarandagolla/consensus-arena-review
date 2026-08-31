import { CheckCircle2 } from 'lucide-react'
import { useAppStore } from '@/stores/useAppStore'

export default function Toast(){const {toasts,removeToast}=useAppStore();if(!toasts.length)return null;return <div className="toast-wrap">{toasts.map(t=><button className="toast" key={t.id} onClick={()=>removeToast(t.id)}><CheckCircle2 size={15} color="var(--green)"/><span>{t.text}</span></button>)}</div>}
