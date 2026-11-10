const KEY = 'update_auto_check'

/** Whether the app asks for a newer release when it starts. On unless switched off. */
export function autoCheckEnabled(): boolean {
  try {
    return localStorage.getItem(KEY) !== 'off'
  } catch {
    return true
  }
}

export function setAutoCheckEnabled(enabled: boolean) {
  try {
    localStorage.setItem(KEY, enabled ? 'on' : 'off')
  } catch {
    // The choice then lasts only for this run.
  }
}
