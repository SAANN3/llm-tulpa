import {useEffect, useRef} from 'react'

/** Open overlays, oldest first — Escape closes only the last one, so a popup opened from
 * another popup doesn't take its parent down with it. */
const openOverlays: object[] = []

/** Calls `onClose` when Escape is pressed while this overlay is the topmost open one */
export const useEscapeToClose = (open: boolean, onClose: () => void) => {
    // Callers pass fresh inline closures; reading the latest through a ref keeps the
    // overlay's place in the stack from being reshuffled on every re-render.
    const onCloseRef = useRef(onClose)
    useEffect(() => {
        onCloseRef.current = onClose
    })

    useEffect(() => {
        if (!open) return
        const token = {}
        openOverlays.push(token)

        const handleKeyDown = (e: KeyboardEvent) => {
            if (e.key !== 'Escape' || openOverlays[openOverlays.length - 1] !== token) return
            e.preventDefault()
            onCloseRef.current()
        }

        document.addEventListener('keydown', handleKeyDown)
        return () => {
            document.removeEventListener('keydown', handleKeyDown)
            openOverlays.splice(openOverlays.indexOf(token), 1)
        }
    }, [open])
};
