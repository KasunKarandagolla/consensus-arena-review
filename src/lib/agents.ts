export const AGENT_IDS = [
  'chatgpt', 'claude', 'gemini', 'deepseek', 'qwen', 'glm', 'kimi',
] as const

export type AgentId = typeof AGENT_IDS[number]

export const AGENT_DISPLAY_NAMES: Record<AgentId, string> = {
  chatgpt:  'ChatGPT',
  claude:   'Claude',
  gemini:   'Gemini',
  deepseek: 'DeepSeek',
  qwen:     'Qwen',
  glm:      'GLM',
  kimi:     'Kimi',
}

export const AGENT_SHORT_NAMES: Record<AgentId, string> = {
  chatgpt:  'GPT',
  claude:   'Claude',
  gemini:   'Gemini',
  deepseek: 'DS',
  qwen:     'Qwen',
  glm:      'GLM',
  kimi:     'Kimi',
}

export function displayName(agentId: string): string {
  return AGENT_DISPLAY_NAMES[agentId as AgentId] ?? agentId
}

export function shortName(agentId: string): string {
  return AGENT_SHORT_NAMES[agentId as AgentId] ?? agentId
}
