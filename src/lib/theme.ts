export type Theme = 'blue' | 'light' | 'dark'

export const THEME_STORAGE_KEY = 'ca-theme'
const VALID_THEMES: readonly Theme[] = ['blue', 'light', 'dark']

export function isValidTheme(value: string | null): value is Theme {
  return value !== null && (VALID_THEMES as readonly string[]).includes(value)
}

export function applyTheme(theme: Theme): void {
  document.documentElement.setAttribute('data-theme', theme)
}

export function loadStoredTheme(): Theme {
  const saved = localStorage.getItem(THEME_STORAGE_KEY)
  // Pre-redesign builds used "default" for the blue palette.
  if (saved === 'default') return 'blue'
  return isValidTheme(saved) ? saved : 'blue'
}

export function storeTheme(theme: Theme): void {
  localStorage.setItem(THEME_STORAGE_KEY, theme)
}
