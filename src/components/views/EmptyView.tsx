import { Code2, Database, Shield, Zap } from 'lucide-react'
import InputBar from '@/components/shared/InputBar'
import Topbar from '@/components/layout/Topbar'
import { useAppStore } from '@/stores/useAppStore'

const suggestions = [
  ['Design an MVP: ', 'Design an MVP', Zap], ['Review a schema: ', 'Review a schema', Database],
  ['Security audit: ', 'Security audit', Shield], ['API design: ', 'API design', Code2],
] as const

export default function EmptyView() {
  const setSetupBrief = useAppStore(s => s.setSetupBrief)
  return <section className="view"><Topbar/><div className="empty">
    <div className="orb-field"><div className="orb orb-1"/><div className="orb orb-2"/><div className="orb orb-3"/></div>
    <div className="hello-wrap">
      <svg className="hello-svg" viewBox="0 186 500 145" role="img" aria-label="hello">
        <defs><linearGradient id="hello-gradient" x1="115" y1="265" x2="386.5" y2="243" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#4D7FFF"/><stop offset=".125" stopColor="#6A72F5"/><stop offset=".25" stopColor="#8865EC"/><stop offset=".375" stopColor="#8769E8"/><stop offset=".5" stopColor="#4D8CEF"/><stop offset=".625" stopColor="#13AFF6"/><stop offset=".75" stopColor="#00C5EC"/><stop offset=".875" stopColor="#00D5D8"/><stop offset="1" stopColor="#00E5C4"/>
        </linearGradient></defs>
        <g transform="translate(250.5 252.4) scale(1.08654) translate(-250 -250)"><path className="hello-path" transform="translate(252 245.918)" pathLength="100" d="M-145.66 43.747 C-145.66 43.747 -86.107 10.264 -81.851 -26.162 C-79.424 -46.943 -98.573 -44.137 -101.426 -23.013 C-103.757 -5.755 -109.596 40.561 -109.596 40.561 C-109.596 40.561 -103.979 -.034 -85.851 1.753 C-65.936 4.083 -91.979 40.05 -69 40.305 C-48.573 40.532 -27.639 22.688 -26.873 10.943 C-25.99 -2.599 -44.362 -4.886 -50.022 11.966 C-55.226 27.461 -43.584 44.902 -23.54 40.581 C7.341 33.922 22.483 -10.827 23.936 -26.077 C25.467 -42.162 13.723 -43.694 6.574 -29.397 C-.104 -16.04 -11.245 37.085 12.958 41.583 C41.809 46.944 64.277 -5.906 67.086 -23.779 C69.802 -41.066 58.656 -45.952 50.234 -30.673 C41.166 -14.223 27.843 44.077 59.937 41.326 C86.746 39.028 76.916 2.264 102.898 -.05 C114.562 -1.088 119.386 9.92 118.532 21.029 C117.638 32.646 106.66 42.475 95.809 40.943 C85.898 39.544 80.838 25.973 83.425 17.072 C86.617 6.094 96.662 .12 102.898 -.05 C111.766 -.29 116.234 5.327 124.149 5.199 C131.179 5.086 138.27 -2.922 138.27 -2.922"/></g>
      </svg>
      <p className="hello-sub">Your panel is assembled. Brief them and step back.</p>
      <div className="suggestions">{suggestions.map(([value,label,Icon]) => <button className="sug" key={label} onClick={() => setSetupBrief(value)}><Icon size={13}/>{label}</button>)}</div>
    </div>
  </div><InputBar/></section>
}
