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

/** Whether a key went to something the user types into, where it means a character and not a command */
export const isTyping = (e: KeyboardEvent): boolean => {
    const target = e.target as HTMLElement | null
    if (!target) return false
    return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)
};
