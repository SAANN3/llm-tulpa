import {useEffect, useState} from 'react'

/** A running time that counts up on its own between the backend's reports: the value reported (seconds,
 * or null when nothing runs) is the anchor and the clock adds the time since it arrived. */
export const useUptime = (reported: number | null): number | null => {
    const [anchor, setAnchor] = useState<{ secs: number; at: number } | null>(null)
    const [, setTick] = useState(0)
    const running = reported != null

    useEffect(() => {
        setAnchor(reported == null ? null : {secs: reported, at: Date.now()})
    }, [reported])

    useEffect(() => {
        if (!running) return
        const id = setInterval(() => setTick((t) => t + 1), 1000)
        return () => clearInterval(id)
    }, [running])

    if (reported == null) return null
    // Until the anchor effect has run, the reported value itself is the best answer
    return anchor ? anchor.secs + (Date.now() - anchor.at) / 1000 : reported
};
