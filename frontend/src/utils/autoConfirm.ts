const STORAGE_KEY = 'llm-tulpa:auto_confirm'

/** Whether "auto-confirm" mode is on — purely a local, per-browser preference (not
 * synced through the backend `Settings` the way `notifications_enabled` is), read
 * fresh wherever it's needed rather than pushed through a shared context, since only
 * one of Settings (writes it) or Chat (reads it) is ever mounted at a time. */
export function getAutoConfirm(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'true'
  } catch {
    return false
  }
}

export function setAutoConfirm(enabled: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(enabled))
  } catch {
    // Private window, blocked site data, ... — best-effort only, same as every other
    // localStorage use in this app; the toggle just won't remember itself next load.
  }
}
