import axios from 'axios'
import {useCallback, useEffect, useRef, useState} from 'react'
import {cancelSttSession} from '../api/stt/cancel-session.ts'
import {finishSttSession} from '../api/stt/finish-session.ts'
import {sendSttAudio} from '../api/stt/send-audio.ts'
import {startSttSession} from '../api/stt/start-session.ts'
import {debugLog} from '../utils/debug.ts'
import {MICROPHONE_AUDIO} from '../utils/microphone.ts'
import workletUrl from '../utils/pcm-capture.ts?worker&url'
import {PCM_FLUSH, type PcmCaptureMessage} from '../utils/pcm-capture-messages.ts'

export type SpeechInputState = 'idle' | 'starting' | 'listening' | 'finishing'

/** What the speech model wants */
const SAMPLE_RATE = 16000
/** The backend takes five minutes of audio at most; the input ends itself a little before */
const MAX_LISTEN_MS = 290_000
/** How long the last, part-filled chunk is given to arrive from the audio thread when the input ends */
const FLUSH_MS = 80
/** How often the level meter follows the microphone, at most (the worklet reports the level every 60 ms) */
const LEVEL_MS = 120
/** The levels kept for the graph: more than it is wide, so a wide screen is never short of them */
const MAX_LEVELS = 400

/** One voice input in progress */
interface Run {
    session: string
    context: AudioContext
    stream: MediaStream
    node: AudioWorkletNode
    source: MediaStreamAudioSourceNode
    /** Chunks taken from the microphone and not yet sent */
    pending: ArrayBuffer[]
    /** A request is in flight: chunks go one at a time, so the texts come back in order */
    sending: boolean
    /** Over: what the answers say no longer matters */
    ended: boolean
    timer: number
    /** The text the last answer gave, to say how much the next one changes of it */
    shown: string
    /** Bytes of audio sent so far: 16 kHz 16-bit, so 32 bytes to a millisecond */
    sentBytes: number
}

const BYTES_PER_MS = 32
/** How much of a changed stretch of text a log line quotes */
const QUOTE_CHARS = 60

const quote = (text: string): string => JSON.stringify(text.length > QUOTE_CHARS ? `${text.slice(0, QUOTE_CHARS)}…` : text)

/** Says in the debug log how an answer differs from the one before: the text that stayed, what it dropped from the end
 * and what it put there instead (the text is a new reading of the audio each time, so it can rewrite its own tail) */
const logAnswer = (r: Run, text: string, tookMs: number, final: boolean) => {
    let kept = 0
    while (kept < r.shown.length && kept < text.length && r.shown[kept] === text[kept]) kept++
    const removed = r.shown.slice(kept)
    const added = text.slice(kept)
    debugLog('voice', `${final ? 'final' : 'answer'} after ${Math.round(r.sentBytes / BYTES_PER_MS)} ms of audio, ${Math.round(tookMs)} ms to answer: ${text.length} chars, kept ${kept}, dropped ${removed.length}, added ${added.length}`,
        removed.length > 0 || added.length > 0 ? `${removed.length > 0 ? `-${quote(removed)} ` : ''}${added.length > 0 ? `+${quote(added)}` : ''}` : 'unchanged')
    r.shown = text
}

const concat = (chunks: ArrayBuffer[]): ArrayBuffer => {
    const out = new Uint8Array(chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0))
    let at = 0
    for (const chunk of chunks) {
        out.set(new Uint8Array(chunk), at)
        at += chunk.byteLength
    }
    return out.buffer
}

/** What went wrong, in words for the user */
const describe = (e: unknown): string => {
    if (axios.isAxiosError(e)) return (e.response?.data as { error?: string } | undefined)?.error ?? e.message
    if (e instanceof DOMException && e.name === 'NotAllowedError') return 'The microphone is blocked. Allow it for this site in the browser\'s site settings, then try again.'
    if (e instanceof DOMException && e.name === 'NotFoundError') return 'No microphone was found.'
    return e instanceof Error ? e.message : String(e)
}

export interface SpeechInput {
    state: SpeechInputState
    /** How loud the last moment was, 0 to 1 */
    level: number
    /** How loud each 60 ms of this input was, oldest first; empty when nothing is being heard */
    levels: number[]
    /** When the input started listening, on the page's clock; null when it is not */
    startedAt: number | null
    /** Why the last try failed; cleared by the next */
    error: string | null
    /** Starts listening, or ends it and takes the whole text */
    toggle: () => void
    /** Ends it and drops what is left unread */
    cancel: () => void
}

/**
 * Voice input: listens to the microphone and sends the audio to the backend as it comes, in half-second chunks;
 * each answer is the text so far, handed to `onText` (`final` for the last, whole one). The backend reads the audio
 * again as it grows, so the text appears and settles while the user speaks. Needs a secure page (https or localhost).
 */
export const useSpeechInput = (onText: (text: string, final: boolean) => void): SpeechInput => {
    const [state, setState] = useState<SpeechInputState>('idle')
    const [level, setLevel] = useState(0)
    const [levels, setLevels] = useState<number[]>([])
    const [startedAt, setStartedAt] = useState<number | null>(null)
    const history = useRef<number[]>([])
    const [error, setError] = useState<string | null>(null)
    const run = useRef<Run | null>(null)
    const onTextRef = useRef(onText)
    const lastLevelAt = useRef(0)
    useEffect(() => {
        onTextRef.current = onText
    })

    /** Lets go of the microphone and the audio context */
    const stopCapture = useCallback((r: Run) => {
        r.ended = true
        window.clearTimeout(r.timer)
        r.node.port.onmessage = null
        r.source.disconnect()
        r.stream.getTracks().forEach((track) => track.stop())
        void r.context.close()
        setLevel(0)
        history.current = []
        setLevels([])
        setStartedAt(null)
    }, [])

    const fail = useCallback((r: Run, e: unknown) => {
        debugLog('voice', 'voice input failed', e)
        setError(describe(e))
        if (!r.ended) stopCapture(r)
        run.current = null
        setState('idle')
    }, [stopCapture])

    /** Sends what has been taken from the microphone, until nothing is left */
    const pump = useCallback(async (r: Run) => {
        if (r.sending) return
        r.sending = true
        try {
            while (r.pending.length > 0 && !r.ended) {
                const chunk = concat(r.pending)
                r.pending = []
                r.sentBytes += chunk.byteLength
                const asked = performance.now()
                const text = await sendSttAudio(r.session, chunk)
                if (!r.ended) {
                    logAnswer(r, text, performance.now() - asked, false)
                    onTextRef.current(text, false)
                }
            }
        } catch (e) {
            if (!r.ended) fail(r, e)
        } finally {
            r.sending = false
        }
    }, [fail])

    const finish = useCallback(async () => {
        const r = run.current
        if (!r || r.ended) return
        setState('finishing')
        // The worklet holds up to half a second that is not a whole chunk yet: it hands that over before the capture stops
        r.node.port.postMessage(PCM_FLUSH)
        await new Promise((resolve) => setTimeout(resolve, FLUSH_MS))
        // From here the pump sends nothing more and what its request in flight answers is not shown; the chunks still
        // waiting go out below, then the voice input is closed and the whole text comes back
        stopCapture(r)
        try {
            while (r.sending) await new Promise((resolve) => setTimeout(resolve, 30))
            if (r.pending.length > 0) {
                const chunk = concat(r.pending)
                r.sentBytes += chunk.byteLength
                await sendSttAudio(r.session, chunk)
            }
            const asked = performance.now()
            const whole = await finishSttSession(r.session)
            logAnswer(r, whole, performance.now() - asked, true)
            onTextRef.current(whole, true)
        } catch (e) {
            setError(describe(e))
        } finally {
            run.current = null
            setState('idle')
        }
    }, [stopCapture])

    const cancel = useCallback(() => {
        const r = run.current
        if (!r) return
        stopCapture(r)
        void cancelSttSession(r.session).catch(() => undefined)
        run.current = null
        setState('idle')
    }, [stopCapture])

    const start = useCallback(async () => {
        setError(null)
        setState('starting')
        // Made in the click itself: a browser lets audio start only from a user's action
        let context: AudioContext
        try {
            context = new AudioContext({sampleRate: SAMPLE_RATE})
        } catch {
            // A browser that runs its audio at one rate: the worklet brings the samples down to 16 kHz
            context = new AudioContext()
        }
        let stream: MediaStream | null = null
        let session: string | null = null
        try {
            if (!navigator.mediaDevices?.getUserMedia) {
                throw new Error('This page cannot record: the browser only allows the microphone on a secure (https) page.')
            }
            stream = await navigator.mediaDevices.getUserMedia({audio: MICROPHONE_AUDIO})
            await context.audioWorklet.addModule(workletUrl)
            session = (await startSttSession()).id
            const source = context.createMediaStreamSource(stream)
            const node = new AudioWorkletNode(context, 'pcm-capture')
            source.connect(node)
            const r: Run = {session, context, stream, node, source, pending: [], sending: false, ended: false, timer: 0, shown: '', sentBytes: 0}
            node.port.onmessage = (message: MessageEvent<PcmCaptureMessage>) => {
                if (r.ended) return
                if (message.data.pcm) r.pending.push(message.data.pcm)
                const peak = Math.min(1, message.data.peak)
                history.current.push(peak)
                if (history.current.length > MAX_LEVELS) history.current.shift()
                const now = performance.now()
                if (now - lastLevelAt.current >= LEVEL_MS) {
                    lastLevelAt.current = now
                    setLevel(peak)
                    setLevels(history.current.slice())
                }
                void pump(r)
            }
            r.timer = window.setTimeout(() => void finish(), MAX_LISTEN_MS)
            run.current = r
            setStartedAt(Date.now())
            setState('listening')
        } catch (e) {
            debugLog('voice', 'voice input did not start', e)
            stream?.getTracks().forEach((track) => track.stop())
            void context.close()
            if (session) void cancelSttSession(session).catch(() => undefined)
            setError(describe(e))
            setState('idle')
        }
    }, [finish, pump])

    const toggle = useCallback(() => {
        if (state === 'idle') void start()
        else if (state === 'listening') void finish()
    }, [state, start, finish])

    // Leaving the page while listening: let go of the microphone
    useEffect(() => () => {
        const r = run.current
        if (!r) return
        stopCapture(r)
        void cancelSttSession(r.session).catch(() => undefined)
        run.current = null
    }, [stopCapture])

    return {state, level, levels, startedAt, error, toggle, cancel}
};
