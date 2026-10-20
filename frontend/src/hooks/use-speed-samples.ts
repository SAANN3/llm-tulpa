import {useEffect, useRef, useState} from 'react'
import type {CallKind, CallStatus} from '../api/agent/types.ts'

/** How many points are kept: more than a graph is wide, so a wide screen is never short of them */
const MAX_POINTS = 400
/** Two reports closer than this add nothing the graph could show */
const MIN_POINT_MS = 120

export interface SpeedPoint {
    /** Milliseconds on the page's clock */
    at: number
    /** Tokens written by the call at that moment */
    tokens: number
}

/**
 * How many tokens the call in flight had written at each of its reports, oldest first. The server's own speed figure is
 * the average over the whole call, which hides a reply slowing down as it goes, and the speed between two neighbouring
 * reports is all jitter (a report can arrive late and the next carry two), so the graph works the speed out over a
 * window of these points. Starts again with every call.
 */
export const useSpeedSamples = (call: CallStatus | null): SpeedPoint[] => {
    const [points, setPoints] = useState<SpeedPoint[]>([])
    const last = useRef<{ kind: CallKind; tokens: number; at: number } | null>(null)
    const kind = call?.kind
    const tokens = call?.generation?.tokens

    useEffect(() => {
        if (kind == null || tokens == null) {
            last.current = null
            setPoints((now) => (now.length ? [] : now))
            return
        }
        const now = performance.now()
        const before = last.current
        if (before == null || before.kind !== kind || tokens < before.tokens) {
            last.current = {kind, tokens, at: now}
            setPoints([{at: now, tokens}])
            return
        }
        if (now - before.at < MIN_POINT_MS) return
        last.current = {kind, tokens, at: now}
        setPoints((prev) => [...prev, {at: now, tokens}].slice(-MAX_POINTS))
    }, [kind, tokens])

    return points
}
