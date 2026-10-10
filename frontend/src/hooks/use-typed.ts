import {useEffect, useRef, useState} from 'react'

/** How far behind the text the typing may fall before it hurries: it types a sixth of what is waiting each frame */
const CATCH_UP = 6

/** Text longer than this when the component mounts was written before the page saw it (a reply joined midway after a
 * reload): it shows at once instead of being typed again from the start */
const JOINED = 200

/**
 * How much of `text` to show for it to read as typed at an even pace, where it actually grows in bursts (a reply's
 * pieces come a few times a second, several words each). Each frame shows at least one more character, and a sixth of
 * what is still waiting, so it never lags far behind. Text longer than `JOINED` when the component mounts (a reply
 * joined midway) shows at once, as does everything with "reduce motion" on; text that is replaced rather than extended
 * starts over.
 */
export const useTyped = (text: string): string => {
    const start = text.length > JOINED ? text.length : 0
    const [shown, setShown] = useState(start)
    const textRef = useRef(text)
    const shownRef = useRef(start)
    const reduced = typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches

    // Replaced, not extended (a reply asked for again): what was typed of the old one doesn't belong to the new one
    if (!text.startsWith(textRef.current.slice(0, shownRef.current))) shownRef.current = 0
    textRef.current = text

    useEffect(() => {
        if (reduced) {
            shownRef.current = text.length
            setShown(text.length)
            return
        }
        if (shownRef.current >= text.length) {
            shownRef.current = text.length
            setShown(text.length)
            return
        }
        let frame = 0
        const step = () => {
            const behind = textRef.current.length - shownRef.current
            if (behind <= 0) return
            shownRef.current += Math.max(1, Math.ceil(behind / CATCH_UP))
            setShown(shownRef.current)
            frame = requestAnimationFrame(step)
        }
        frame = requestAnimationFrame(step)
        return () => cancelAnimationFrame(frame)
    }, [text, reduced])

    return text.slice(0, Math.min(shown, shownRef.current, text.length))
}
