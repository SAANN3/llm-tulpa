import {useEffect, useRef} from 'react'

/** Whatever handles keys right now, oldest first: a page's panel at the bottom, each popup opened over it above.
 * Only the newest one hears a key, so a popup opened from another popup (or over a page) takes the keyboard
 * without its parent reacting too, and gives it back when it closes. */
const layers: object[] = []

/** Handles one key press; returns whether it did, which keeps the browser's own reaction to it from running */
export type KeyHandler = (e: KeyboardEvent) => boolean

/** Gives `handler` the keyboard while `active` and nothing newer holds it */
export const useKeyLayer = (active: boolean, handler: KeyHandler) => {
    // Callers pass fresh inline closures; reading the latest through a ref keeps the layer's place in the
    // stack from being reshuffled on every re-render
    const handlerRef = useRef(handler)
    useEffect(() => {
        handlerRef.current = handler
    })

    useEffect(() => {
        if (!active) return
        const token = {}
        layers.push(token)

        const onKeyDown = (e: KeyboardEvent) => {
            if (layers[layers.length - 1] !== token) return
            if (handlerRef.current(e)) e.preventDefault()
        }

        document.addEventListener('keydown', onKeyDown)
        return () => {
            document.removeEventListener('keydown', onKeyDown)
            layers.splice(layers.indexOf(token), 1)
        }
    }, [active])
};

const TEXT_INPUT_TYPES = new Set(['text', 'password', 'search', 'email', 'url', 'tel', 'number'])
const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight']

/** The keys a focused control uses for itself, by its input type or role; every other key goes on to the page, so a
 * switch that was just clicked (and keeps the focus) doesn't stop the tab keys or Escape from working */
const OWN_KEYS: Record<string, string[]> = {
    checkbox: [' '],
    radio: [...ARROWS, ' '],
    range: [...ARROWS, 'Home', 'End', 'PageUp', 'PageDown'],
}

/** Whether `el` is something the user types into, where every key means a character */
export const isTextEntry = (el: Element | null): boolean => {
    if (!(el instanceof HTMLElement)) return false
    if (el.isContentEditable || el.tagName === 'TEXTAREA') return true
    return el.tagName === 'INPUT' && TEXT_INPUT_TYPES.has((el as HTMLInputElement).type)
};

/** Whether a key belongs to the focused control rather than the page: any key in a text field (or a native select),
 * and a control's own keys (`OWN_KEYS`) on it */
export const keyBelongsToFocus = (e: KeyboardEvent): boolean => {
    const target = e.target as HTMLElement | null
    if (!target) return false
    if (isTextEntry(target) || target.tagName === 'SELECT') return true
    const kind = target.tagName === 'INPUT' ? (target as HTMLInputElement).type : target.getAttribute('role') ?? ''
    return OWN_KEYS[kind]?.includes(e.key) ?? false
};
