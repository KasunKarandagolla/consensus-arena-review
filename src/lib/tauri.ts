/// <reference types="vite/client" />
import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { listen as tauriListen, type EventCallback, type UnlistenFn } from '@tauri-apps/api/event'

/**
 * True only when running inside an actual Tauri webview.
 * False in a plain browser tab (e.g. `npm run dev` opened directly,
 * without `npm run tauri dev`).
 */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

export function errorToString(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message || error.name
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === 'string') return message
  }
  try {
    const serialized = JSON.stringify(error)
    return serialized && serialized !== '{}' ? serialized : String(error)
  } catch {
    return String(error)
  }
}

export function redactSecrets(value: string): string {
  return value
    .replace(/\bBearer\s+[^\s,;"']+/gi, 'Bearer [REDACTED]')
    .replace(
      /(["']?)(api[_-]?key|apiKey)\1(\s*[:=]\s*)(["']?)([^\s,"'}]+)\4/gi,
      (_match, fieldQuote: string, field: string, separator: string, valueQuote: string) =>
        `${fieldQuote}${field}${fieldQuote}${separator}${valueQuote}[REDACTED]${valueQuote}`,
    )
    .replace(/\b(?:sk|pk|key|token)-[A-Za-z0-9._-]{12,}\b/gi, '[REDACTED]')
    .replace(/[A-Za-z0-9_+./=-]{32,}/g, (candidate) => {
      const looksLikeSecret = /[A-Za-z]/.test(candidate) && /\d/.test(candidate)
      const looksLikePathOrUrl = candidate.includes('/')
      return looksLikeSecret && !looksLikePathOrUrl ? '[REDACTED]' : candidate
    })
}

export function buildCommandErrorMessage(commandName: string, error: unknown): string {
  const detail = redactSecrets(errorToString(error)).trim() || 'Unknown backend error'
  return `${commandName} failed: ${detail}`
}

/**
 * Drop-in replacement for @tauri-apps/api/core's invoke().
 * Outside a Tauri webview, resolves to undefined instead of throwing —
 * so a useEffect calling this on mount never crashes the React tree.
 * Inside Tauri, behaves identically to the real invoke().
 */
export async function safeInvoke<T = unknown>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) {
    console.debug(`[tauri-stub] invoke("${cmd}") skipped — not running inside Tauri`)
    return undefined as unknown as T
  }
  return tauriInvoke<T>(cmd, args)
}

/**
 * Drop-in replacement for @tauri-apps/api/event's listen().
 * Outside a Tauri webview, returns a no-op unlisten function instead of
 * throwing — so useEffect cleanup still runs correctly.
 * Inside Tauri, behaves identically to the real listen().
 */
export async function safeListen<T = unknown>(
  event: string,
  handler: EventCallback<T>,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    console.debug(`[tauri-stub] listen("${event}") skipped — not running inside Tauri`)
    return () => {}
  }
  return tauriListen<T>(event, handler)
}
