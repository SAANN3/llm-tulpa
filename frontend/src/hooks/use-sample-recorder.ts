import {useCallback, useEffect, useRef, useState} from 'react'
import {debugLog} from '../utils/debug.ts'
import {MICROPHONE_AUDIO} from '../utils/microphone.ts'
import workletUrl from '../utils/pcm-capture.ts?worker&url'
import {PCM_FLUSH, type PcmCaptureMessage} from '../utils/pcm-capture-messages.ts'

export type SampleRecorderState = 'idle' | 'starting' | 'recording'

/** What the speech models want */
const SAMPLE_RATE = 16000
/** The backend measures on 3 to 15 seconds of speech: the recording ends itself at the longest */
export const MAX_SAMPLE_SECONDS = 15
export const MIN_SAMPLE_SECONDS = 3
/** How long the last, part-filled chunk is given to arrive from the audio thread when the recording ends */
const FLUSH_MS = 80
/** The levels kept for the graph: more than it is wide, so a wide screen is never short of them */
const MAX_LEVELS = 400
/** How often the graph follows the microphone, at most (the worklet reports the level every 60 ms) */
const LEVEL_MS = 120
const BYTES_PER_SECOND = SAMPLE_RATE * 2

interface Capture {
    context: AudioContext
    stream: MediaStream
    source: MediaStreamAudioSourceNode
    node: AudioWorkletNode
    chunks: ArrayBuffer[]
    timer: number
}

export interface SampleRecorder {
    state: SampleRecorderState
    /** How loud each 60 ms was, oldest first, for a graph while recording */
    levels: number[]
    startedAt: number | null
    /** The recording once it has ended: 16-bit little-endian mono at 16 kHz; null before one and after `discard` */
    sample: ArrayBuffer | null
    /** How long the recording is, in seconds */
    seconds: number
    error: string | null
    start: () => void
    stop: () => void
    discard: () => void
}

/**
 * Records a few seconds of the user's voice into memory as the speech models want it (nothing is sent anywhere until
 * the page does that): the sample the speed measurement runs on. The same microphone, worklet and settings as the voice
 * input, without a session.
 */
export const useSampleRecorder = (): SampleRecorder => {
    const [state, setState] = useState<SampleRecorderState>('idle')
    const [levels, setLevels] = useState<number[]>([])
    const [startedAt, setStartedAt] = useState<number | null>(null)
    const [sample, setSample] = useState<ArrayBuffer | null>(null)
    const [error, setError] = useState<string | null>(null)
    const capture = useRef<Capture | null>(null)
    const history = useRef<number[]>([])
    const lastLevelAt = useRef(0)

    const release = useCallback((c: Capture) => {
        window.clearTimeout(c.timer)
        c.node.port.onmessage = null
        c.source.disconnect()
        c.stream.getTracks().forEach((track) => track.stop())
        void c.context.close()
        capture.current = null
        history.current = []
        setLevels([])
        setStartedAt(null)
    }, [])

    const stop = useCallback(async () => {
        const c = capture.current
        if (!c) return
        // The worklet holds a part-filled chunk: it hands that over before the capture is let go of
        c.node.port.postMessage(PCM_FLUSH)
        await new Promise((resolve) => setTimeout(resolve, FLUSH_MS))
        const bytes = new Uint8Array(c.chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0))
        let at = 0
        for (const chunk of c.chunks) {
            bytes.set(new Uint8Array(chunk), at)
            at += chunk.byteLength
        }
        release(c)
        setSample(bytes.buffer)
        setState('idle')
    }, [release])

    const start = useCallback(async () => {
        setError(null)
        setSample(null)
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
        try {
            if (!navigator.mediaDevices?.getUserMedia) {
                throw new Error('This page cannot record: the browser only allows the microphone on a secure (https) page.')
            }
            stream = await navigator.mediaDevices.getUserMedia({audio: MICROPHONE_AUDIO})
            await context.audioWorklet.addModule(workletUrl)
            const source = context.createMediaStreamSource(stream)
            const node = new AudioWorkletNode(context, 'pcm-capture')
            source.connect(node)
            const c: Capture = {context, stream, source, node, chunks: [], timer: 0}
            node.port.onmessage = (message: MessageEvent<PcmCaptureMessage>) => {
                if (message.data.pcm) c.chunks.push(message.data.pcm)
                history.current.push(Math.min(1, message.data.peak))
                if (history.current.length > MAX_LEVELS) history.current.shift()
                const now = performance.now()
                if (now - lastLevelAt.current >= LEVEL_MS) {
                    lastLevelAt.current = now
                    setLevels(history.current.slice())
                }
            }
            c.timer = window.setTimeout(() => void stop(), MAX_SAMPLE_SECONDS * 1000)
            capture.current = c
            setStartedAt(Date.now())
            setState('recording')
        } catch (e) {
            debugLog('voice', 'the sample could not be recorded', e)
            stream?.getTracks().forEach((track) => track.stop())
            void context.close()
            setError(e instanceof DOMException && e.name === 'NotAllowedError'
                ? 'The microphone is blocked. Allow it for this site in the browser\'s site settings, then try again.'
                : e instanceof Error ? e.message : String(e))
            setState('idle')
        }
    }, [stop])

    const discard = useCallback(() => setSample(null), [])

    // Leaving the page while recording: let go of the microphone
    useEffect(() => () => {
        if (capture.current) release(capture.current)
    }, [release])

    return {
        state, levels, startedAt, sample,
        seconds: sample ? sample.byteLength / BYTES_PER_SECOND : 0,
        error,
        start: () => void start(),
        stop: () => void stop(),
        discard,
    }
};
