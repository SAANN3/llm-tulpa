import {useEffect, useState} from 'react'

/** Seconds since `since`, read from the clock on every tick rather than counted up, and again when the tab is shown: a background tab's timers are throttled, so a counter falls behind real time and a run that is still going looks stalled the moment you tab back in */
export const useSecondsSince = (since: number | null): number => {
    const [now, setNow] = useState(() => Date.now())
    useEffect(() => {
        const tick = () => setNow(Date.now())
        const id = setInterval(tick, 1000)
        document.addEventListener('visibilitychange', tick)
        return () => {
            clearInterval(id)
            document.removeEventListener('visibilitychange', tick)
        }
    }, [])
    return since == null ? 0 : Math.max(0, Math.floor((now - since) / 1000))
}
