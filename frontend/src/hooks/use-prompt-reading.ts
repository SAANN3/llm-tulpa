import {useEffect, useRef, useState} from 'react'
import type {CallStatus} from '../api/agent/types.ts'
import {debugLog} from '../utils/debug.ts'

/** How often the estimate between two reports is refreshed, ms */
const TICK_MS = 50
/** The size of the first stretch the server reads before it reports: its default batch */
const FIRST_STEP_TOKENS = 2048
/** How long the shown progress takes to close most of a gap to where the reports say it is (a report that jumps ahead
 * is glided to, not snapped to), ms */
const EASE_MS = 300
/** The estimate never runs further ahead of the last report than this, however fast the last stretch was */
const MIN_STEP_TOKENS = 512

export interface PromptReading {
    /** What the server has to read: the prompt less the part it already held */
    work: number
    cached: number
    /** Tokens read so far: the last report, carried on at the speed of the last stretch until the next one comes */
    done: number
    /** `null` until the server has reported any progress: with nothing measured there is nothing honest to show */
    percent: number | null
    /** The server's own average over the whole reading, once it has one */
    perSecond: number | null
}

interface Track {
    key: string
    /** The last report, and when it arrived here */
    reportedDone: number
    reportedAt: number
    elapsedMs: number
    /** Tokens per second of the last stretch, which the estimate carries on at */
    rate: number | null
    /** How many tokens the last stretch was: the next one is guessed to be as long */
    step: number
    /** Where the bar is heading: the last report or the estimate past it, whichever is further; never goes back */
    target: number
    /** What the bar shows, gliding to `target`, and when it was last moved */
    shown: number
    movedAt: number
}

/**
 * How far the model server has got reading a prompt, as a page can show it. The server reports once per batch, seconds
 * apart on a long prompt, so a bar that follows the reports alone stands still and then jumps. Between reports the
 * progress is carried on at the speed of the stretch before, up to where the next report is expected, so the bar moves
 * smoothly and each report only corrects it (forward: what is shown never goes back). Before the first report there is no
 * figure at all (`percent` is `null`), and that report is glided to from zero. `null` when the call isn't
 * reading a prompt (none reported, or the reply has begun).
 */
export const usePromptReading = (call: CallStatus | null): PromptReading | null => {
    const track = useRef<Track | null>(null)
    const [, setTick] = useState(0)
    const prompt = call != null && call.generation == null ? call.prompt : null
    const active = prompt != null

    useEffect(() => {
        if (!active) return
        const id = setInterval(() => setTick((n) => n + 1), TICK_MS)
        return () => clearInterval(id)
    }, [active])

    if (prompt == null || call == null) {
        track.current = null
        return null
    }

    const work = Math.max(0, prompt.total - prompt.cached)
    const done = Math.max(0, Math.min(work, prompt.processed - prompt.cached))
    const now = performance.now()
    const key = `${call.kind}:${prompt.total}:${prompt.cached}`
    let t = track.current
    if (t != null && t.key === key && done < t.reportedDone) {
        // The server took the prompt from the start again (a cache it could not use): the bar goes back with it
        debugLog('progress', `${call.kind}: the prompt is read again from the start`, {was: t.reportedDone, now: done, work})
    }
    if (t == null || t.key !== key || done < t.reportedDone) {
        t = {key, reportedDone: done, reportedAt: now, elapsedMs: prompt.elapsed_ms, rate: null, step: FIRST_STEP_TOKENS, target: done, shown: 0, movedAt: now}
        track.current = t
    } else if (done > t.reportedDone) {
        const tokens = done - t.reportedDone
        const ms = prompt.elapsed_ms - t.elapsedMs
        if (ms > 0) {
            t.rate = (tokens / ms) * 1000
        }
        t.step = tokens
        t.reportedDone = done
        t.reportedAt = now
        t.elapsedMs = prompt.elapsed_ms
    }

    const guess = t.rate != null ? t.reportedDone + (t.rate * (now - t.reportedAt)) / 1000 : t.reportedDone
    const ceiling = Math.min(work, t.reportedDone + Math.max(t.step, MIN_STEP_TOKENS))
    t.target = Math.max(t.target, t.reportedDone, Math.min(ceiling, guess))
    // Closes the gap to the target at a pace of its own: the carried-on estimate is smooth already, a report that jumps
    // ahead is not
    t.shown += (t.target - t.shown) * (1 - Math.exp(-(now - t.movedAt) / EASE_MS))
    t.movedAt = now

    return {
        work,
        cached: prompt.cached,
        done: t.shown,
        percent: t.reportedDone <= 0 && t.rate == null ? null : work > 0 ? (t.shown / work) * 100 : 100,
        perSecond: prompt.elapsed_ms > 0 ? (t.reportedDone / prompt.elapsed_ms) * 1000 : null,
    }
}
