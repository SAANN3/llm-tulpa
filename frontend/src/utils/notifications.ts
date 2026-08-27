/**
 * Asks the browser for notification permission — must be called from a user gesture
 * (e.g. the settings toggle being switched on), browsers silently ignore the request
 * otherwise. Returns whether permission actually ended up granted, so the caller can
 * decide whether to persist the toggle as on. `Notification` doesn't exist in every
 * environment (non-browser, or a browser without support), hence the feature check.
 */
export async function requestNotificationPermission(): Promise<boolean> {
  if (!('Notification' in window)) return false
  if (Notification.permission === 'granted') return true
  if (Notification.permission === 'denied') return false

  const result = await Notification.requestPermission()
  return result === 'granted'
}

/**
 * Shows a browser notification with the given `title`/`text`, if permission was
 * actually granted and the tab isn't currently the one the user's looking at — no point
 * notifying about something already visible on screen. Silently does nothing otherwise
 * (permission never granted, revoked since, or unsupported browser). Generic on purpose
 * — callers own what the notification actually says, this just owns whether/how it's
 * allowed to show one.
 */
export function notify(title: string, text: string): void {
  if (!('Notification' in window)) return
  if (Notification.permission !== 'granted') return
  if (!document.hidden) return

  new Notification(title, { body: text })
}
