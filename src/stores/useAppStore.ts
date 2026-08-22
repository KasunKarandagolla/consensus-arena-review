import { create } from 'zustand'

export interface Participant {
  agent_id: string
  display_name: string
  base_url: string
  is_custom: boolean
}

export interface BlueprintSection {
  id: string
  title: string
  content: string
  status: 'draft' | 'agreed' | 'negotiation' | 'disputed'
}

export interface AgentBrainConfig {
  api_key: string
  base_url: string
  model: string
  system_prompt: string
}

export interface AskUserPayload {
  question: string
  options: string[]
  allow_custom: boolean
}

export interface CaptchaPayload {
  agent_id: string
}

export interface RateLimitPayload {
  agent_id: string
  estimated_reset_mins: number
}

export interface RecoveryState {
  available: boolean
  session_id: string
}

export interface ToastMessage {
  id: string
  text: string
  duration?: number
}

export interface AppStore {
  // P3: unified participant registry (built-ins + persisted custom)
  participants: Participant[]
  setParticipants: (participants: Participant[]) => void

  // Session state
  sessionStatus: 'idle' | 'setup' | 'priming' | 'requirements' | 'running' | 'paused' | 'complete' | 'ended'
  setupProgress: string[]
  selectedSessionId: string | null
  recoveryState: RecoveryState | null
  setupBrief: string
  sessionAgentIds: string[]
  setupReadyAgentId: string | null
  setupFailedAgentId: string | null
  activeAgentId: string | null
  activeTurnNumber: number | null

  // Blueprint
  blueprintSections: BlueprintSection[]

  // Live status
  liveStatusText: string
  liveStatusExpanded: boolean
  currentAgentResponse: string

  // Overlays
  askUserPending: AskUserPayload | null
  captchaPending: CaptchaPayload | null
  rateLimitPending: RateLimitPayload | null

  // Toast
  toasts: ToastMessage[]

  // Settings
  agentBrainConfig: AgentBrainConfig | null

  // Settings panel open
  settingsOpen: boolean

  // Batch D: sidebar collapse (mockup's .sidebar.closed state)
  sidebarCollapsed: boolean

  // Actions
  setSessionStatus: (status: AppStore['sessionStatus']) => void
  addSetupProgress: (agentId: string) => void
  clearSetupProgress: () => void
  setSelectedSessionId: (id: string | null) => void
  setRecoveryState: (state: RecoveryState | null) => void
  setSetupBrief: (brief: string) => void
  setSessionAgentIds: (ids: string[]) => void
  setSetupReadyAgentId: (id: string | null) => void
  setSetupFailedAgentId: (id: string | null) => void
  setActiveAgentId: (id: string | null) => void
  setActiveTurn: (agentId: string | null, turnNumber: number | null) => void

  clearSessionState: () => void

  upsertBlueprintSection: (section: BlueprintSection) => void
  appendBlueprintSection: (section: BlueprintSection) => void
  clearBlueprintSections: () => void

  setLiveStatusText: (text: string) => void
  setLiveStatusExpanded: (expanded: boolean) => void
  setCurrentAgentResponse: (text: string) => void

  setAskUserPending: (payload: AskUserPayload | null) => void
  setCaptchaPending: (payload: CaptchaPayload | null) => void
  setRateLimitPending: (payload: RateLimitPayload | null) => void

  addToast: (text: string, duration?: number) => void
  removeToast: (id: string) => void

  setAgentBrainConfig: (config: AgentBrainConfig | null) => void
  setSettingsOpen: (open: boolean) => void

  // Batch D: sidebar collapse toggle (mockup's toggleSB())
  toggleSidebar: () => void
}

export const useAppStore = create<AppStore>((set, get) => ({
  participants: [
    { agent_id: 'chatgpt', display_name: 'ChatGPT', base_url: 'https://chatgpt.com', is_custom: false },
    { agent_id: 'claude', display_name: 'Claude', base_url: 'https://claude.ai', is_custom: false },
    { agent_id: 'gemini', display_name: 'Gemini', base_url: 'https://gemini.google.com', is_custom: false },
    { agent_id: 'deepseek', display_name: 'DeepSeek', base_url: 'https://chat.deepseek.com', is_custom: false },
    { agent_id: 'qwen', display_name: 'Qwen', base_url: 'https://chat.qwen.ai', is_custom: false },
    { agent_id: 'glm', display_name: 'GLM', base_url: 'https://chat.z.ai/', is_custom: false },
    { agent_id: 'kimi', display_name: 'Kimi', base_url: 'https://www.kimi.com/', is_custom: false },
  ],
  setParticipants: (participants) => set({ participants }),

  sessionStatus: 'idle',
  setupProgress: [],
  selectedSessionId: null,
  recoveryState: null,
  setupBrief: '',
  sessionAgentIds: [],
  setupReadyAgentId: null,
  setupFailedAgentId: null,
  activeAgentId: null,
  activeTurnNumber: null,
  blueprintSections: [],
  liveStatusText: '',
  liveStatusExpanded: false,
  currentAgentResponse: '',
  askUserPending: null,
  captchaPending: null,
  rateLimitPending: null,
  toasts: [],
  agentBrainConfig: null,
  settingsOpen: false,
  sidebarCollapsed: false,

  setSessionStatus: (status) => set({ sessionStatus: status }),
  addSetupProgress: (agentId) => set((s) => ({
    setupProgress: s.setupProgress.includes(agentId)
      ? s.setupProgress
      : [...s.setupProgress, agentId],
  })),
  clearSetupProgress: () => set({ setupProgress: [] }),
  setSelectedSessionId: (id) => set({ selectedSessionId: id }),
  setRecoveryState: (state) => set({ recoveryState: state }),
  setSetupBrief: (brief) => set({ setupBrief: brief }),
  setSessionAgentIds: (ids) => set({ sessionAgentIds: ids }),
  setSetupReadyAgentId: (id) => set({ setupReadyAgentId: id }),
  setSetupFailedAgentId: (id) => set({ setupFailedAgentId: id }),
  setActiveAgentId: (id) => set({ activeAgentId: id }),
  setActiveTurn: (agentId, turnNumber) => set({ activeAgentId: agentId, activeTurnNumber: turnNumber }),

  clearSessionState: () => set({
    blueprintSections: [],
    liveStatusText: '',
    liveStatusExpanded: false,
    currentAgentResponse: '',
    setupProgress: [],
    askUserPending: null,
    captchaPending: null,
    rateLimitPending: null,
    setupReadyAgentId: null,
    setupFailedAgentId: null,
    activeAgentId: null,
    activeTurnNumber: null,
  }),

  upsertBlueprintSection: (section) => set((s) => {
    const idx = s.blueprintSections.findIndex(sec => sec.id === section.id)
    if (idx >= 0) {
      const updated = [...s.blueprintSections]
      updated[idx] = section
      return { blueprintSections: updated }
    }
    return { blueprintSections: [...s.blueprintSections, section] }
  }),
  appendBlueprintSection: (section) => set((s) => ({
    blueprintSections: [...s.blueprintSections, section],
  })),
  clearBlueprintSections: () => set({ blueprintSections: [] }),

  setLiveStatusText: (text) => set({ liveStatusText: text }),
  setLiveStatusExpanded: (expanded) => set({ liveStatusExpanded: expanded }),
  setCurrentAgentResponse: (text) => set({ currentAgentResponse: text }),

  setAskUserPending: (payload) => set({ askUserPending: payload }),
  setCaptchaPending: (payload) => set({ captchaPending: payload }),
  setRateLimitPending: (payload) => set({ rateLimitPending: payload }),

  addToast: (text, duration = 3000) => {
    const id = Date.now().toString()
    set((s) => ({ toasts: [...s.toasts, { id, text, duration }] }))
    setTimeout(() => get().removeToast(id), duration)
  },
  removeToast: (id) => set((s) => ({
    toasts: s.toasts.filter((t) => t.id !== id),
  })),

  setAgentBrainConfig: (config) => set({ agentBrainConfig: config }),
  setSettingsOpen: (open) => set({ settingsOpen: open }),
  toggleSidebar: () => set((s) => ({ sidebarCollapsed: !s.sidebarCollapsed })),
}))
