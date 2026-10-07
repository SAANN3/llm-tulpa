/**
 * A per-browser switch for diagnostic output: the Settings page sets it, and anything that wants to say what it is
 * doing for a bug hunt checks it first. It lives in this browser's storage rather than in the user's settings so
 * that one tab can log without every device doing it, and it works before anyone has signed in.
 */
const KEY = 'debug'
const CHANGED_EVENT = 'llm-tulpa-debug-changed'

export const isDebug = (): boolean => {
    try {
        return localStorage.getItem(KEY) === '1'
    } catch {
        // Storage can be blocked (private window, site data off): no switch then, so no output
        return false
    }
};

/** The newest lines `debugLog` wrote, for a page that shows them on screen (a phone has no console) */
const MAX_LINES = 80
let lines: readonly string[] = []
const lineListeners = new Set<() => void>()

const setLines = (next: readonly string[]) => {
    lines = next
    lineListeners.forEach((listener) => listener())
}

export const getDebugLines = (): readonly string[] => lines

export const subscribeDebugLines = (onChange: () => void): (() => void) => {
    lineListeners.add(onChange)
    return () => {
        lineListeners.delete(onChange)
    }
};

export const setDebug = (on: boolean): void => {
    if (!on) setLines([])
    try {
        if (on) localStorage.setItem(KEY, '1')
        else localStorage.removeItem(KEY)
    } catch {
        // Nothing to keep it in
    }
    // The `storage` event only reaches other tabs
    window.dispatchEvent(new Event(CHANGED_EVENT))
};

/** What `useSyncExternalStore` needs to follow the switch, from this tab and from the others */
export const subscribeDebug = (onChange: () => void): (() => void) => {
    window.addEventListener(CHANGED_EVENT, onChange)
    window.addEventListener('storage', onChange)
    return () => {
        window.removeEventListener(CHANGED_EVENT, onChange)
        window.removeEventListener('storage', onChange)
    }
};

const describe = (detail: unknown): string => {
    if (detail instanceof Error) return detail.message
    return typeof detail === 'string' ? detail : JSON.stringify(detail) ?? String(detail)
}

/** Writes one line when the debug switch is on, to the console and to what `getDebugLines` returns: `[scope] time visibility details`. The visibility is there because a background tab behaves differently */
export const debugLog = (scope: string, ...details: unknown[]): void => {
    if (!isDebug()) return
    const now = new Date()
    console.log(`[${scope}] ${now.toISOString()} ${document.visibilityState}`, ...details)
    const time = now.toISOString().slice(11, 23)
    const visible = document.visibilityState === 'visible' ? 'v' : 'h'
    setLines([...lines.slice(-(MAX_LINES - 1)), `${time} ${visible} ${scope} ${details.map(describe).join(' ').trim()}`])
};
