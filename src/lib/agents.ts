import { useAppStore, type Participant } from '@/stores/useAppStore'
import { safeInvoke } from '@/lib/tauri'

// The seven immutable built-in participants, in frozen order. These mirror the
// backend AGENTS registry exactly (ids, display names, base URLs) and must
// never change. The unified runtime registry (built-ins + persisted custom
// participants) is loaded from the backend via `get_participants` and held in
// the app store; these static defs back the offline/built-in defaults and the
// `displayName`/`shortName` fallbacks.
export const AGENT_IDS = [
  'chatgpt', 'claude', 'gemini', 'deepseek', 'qwen', 'glm', 'kimi',
] as const

export type AgentId = typeof AGENT_IDS[number]

export const AGENT_DISPLAY_NAMES: Record<string, string> = {
  chatgpt:  'ChatGPT',
  claude:   'Claude',
  gemini:   'Gemini',
  deepseek: 'DeepSeek',
  qwen:     'Qwen',
  glm:      'GLM',
  kimi:     'Kimi',
}

export const AGENT_SHORT_NAMES: Record<string, string> = {
  chatgpt:  'GPT',
  claude:   'Claude',
  gemini:   'Gemini',
  deepseek: 'DS',
  qwen:     'Qwen',
  glm:      'GLM',
  kimi:     'Kimi',
}

/**
 * The unified runtime registry. Calls `get_participants` (a JSON-string IPC
 * command) and stores the parsed merged list in the app store. Returns the
 * merged list so callers can use it directly. Safe outside Tauri (no-op stub).
 */
export async function loadParticipants(): Promise<Participant[]> {
  try {
    const raw = await safeInvoke<string>('get_participants')
    const parsed = JSON.parse(raw ?? '[]') as Participant[]
    if (Array.isArray(parsed) && parsed.length > 0) {
      useAppStore.getState().setParticipants(parsed)
      return parsed
    }
  } catch (error) {
    console.error('[participants] failed to load unified registry', error)
  }
  return useAppStore.getState().participants
}

/** Await a fresh unified registry (used after a custom participant save/delete). */
export async function refreshParticipants(): Promise<Participant[]> {
  return loadParticipants()
}

/** Current unified registry from the app store (built-ins + customs). */
export function getParticipants(): Participant[] {
  return useAppStore.getState().participants
}

/**
 * Resolve a participant's display name across the unified registry. Built-in
 * ids resolve to their frozen display names (identical to before P3); custom
 * ids resolve to their persisted display name via the loaded registry; unknown
 * ids fall back to the raw id (matching the prior fallback for unknown ids).
 * An optional `participants` array can be supplied to override the store.
 */
export function displayName(agentId: string, participants?: Participant[]): string {
  if (AGENT_IDS.includes(agentId as AgentId)) {
    return AGENT_DISPLAY_NAMES[agentId]
  }
  const list = participants ?? useAppStore.getState().participants
  const found = list.find((p) => p.agent_id === agentId)
  return found ? found.display_name : agentId
}

/**
 * Resolve a participant's short name (built-in short names, custom ids, then
 * the raw id fallback). Built-in output is identical to before P3.
 */
export function shortName(agentId: string, participants?: Participant[]): string {
  if (AGENT_IDS.includes(agentId as AgentId)) {
    return AGENT_SHORT_NAMES[agentId]
  }
  const list = participants ?? useAppStore.getState().participants
  const found = list.find((p) => p.agent_id === agentId)
  return found ? found.display_name : agentId
}
