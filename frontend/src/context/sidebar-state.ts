import {useSyncExternalStore} from 'react'

const STORAGE_KEY = 'sidebar_collapsed'
// Window width at which the sidebar auto-collapses; growing the window back past it auto-expands
const COLLAPSE_AT_WIDTH = 900

const readStored = (): boolean => localStorage.getItem(STORAGE_KEY) === 'true'
const writeStored = (collapsed: boolean): void => localStorage.setItem(STORAGE_KEY, String(collapsed))

// A window below the threshold starts collapsed regardless of the stored choice, and only a
// manual toggle is ever written — so a reload on a wide window restores the manual choice
let collapsed = window.innerWidth < COLLAPSE_AT_WIDTH || readStored()
// Whether the last-seen width was below the threshold, so a resize only acts on a crossing —
// a resize that stays on one side never clobbers a manual choice
let lastBelow = window.innerWidth < COLLAPSE_AT_WIDTH

let listeners: (() => void)[] = []

const emit = (): void => {
    for (const listener of listeners) listener()
}

/** Sets the collapsed state (a user action or a threshold crossing) and persists it */
export const setSidebarCollapsed = (value: boolean): void => {
    if (collapsed === value) return
    collapsed = value
    writeStored(value)
    emit()
}

/** Feeds window resizes (the mounted sidebar is the only caller) */
export const onSidebarWindowResized = (width: number): void => {
    const below = width < COLLAPSE_AT_WIDTH
    if (below === lastBelow) return
    lastBelow = below
    setSidebarCollapsed(below)
}

/** Reactive read of the collapsed state — re-renders whoever subscribes when it changes */
export const useSidebarCollapsed = (): boolean =>
    useSyncExternalStore(
        (onChange) => {
            listeners.push(onChange)
            return () => {
                listeners = listeners.filter((listener) => listener !== onChange)
            }
        },
        () => collapsed,
    )
