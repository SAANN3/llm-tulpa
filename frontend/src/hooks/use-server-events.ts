import {useEffect, useRef} from 'react'
import {BACKEND_URL} from '../config'
import type {RunEndReason} from '../api/agent/types'
import {getToken} from '../utils/auth-token'
import {debugLog, isDebug} from '../utils/debug.ts'

export type ServerEvent =
    | { type: 'job_finished'; chat_id: number; job_id: number }
    | { type: 'turn_progress'; chat_id: number; eval_tokens: number; prompt_tokens: number | null; step: number }
    | { type: 'stream_open' }
    | { type: 'run_started'; chat_id: number; parent_chat_id: number | null; started_at: string }
    | { type: 'messages_changed'; chat_id: number }
    | { type: 'tool_started'; chat_id: number; tool_name: string }
    | { type: 'run_ended'; chat_id: number; reason: RunEndReason; detail: string | null; started_at: string; eval_tokens: number; status: number | null }
    | { type: 'chat_seen'; chat_id: number }
    | { type: 'chat_created'; chat_id: number }
    | { type: 'chat_renamed'; chat_id: number; name: string }
    | { type: 'chat_deleted'; chat_id: number }
    | { type: 'messages_removed'; chat_id: number; message_ids: number[] }
    | { type: 'model_state'; state: 'loading' | 'ready' | 'stopped' | 'failed' | 'queued'; profile_id: number | null; model: string | null; detail: string | null }

type Listener = (event: ServerEvent) => void

/** How long the connection stays open after its last subscriber unmounts, so a remount reuses it */
const CLOSE_GRACE_MS = 1000

/** Delay before reconnecting after the stream drops */
const RECONNECT_MS = 3000

const listeners = new Set<Listener>()
let controller: AbortController | null = null
let closeTimer: ReturnType<typeof setTimeout> | null = null

/**
 * One tab per browser holds the stream and passes every event on to the others. A browser opens at most six
 * connections to one server over HTTP/1.1 and every stream keeps one busy for good, so with a stream per tab the
 * seventh tab's stream waited for ever and its page never heard a run end. The tab holding the lock below is the one
 * that connects; when it closes, the browser hands the lock to a waiting tab, which connects in turn. Without the
 * lock API (it needs a secure context: a LAN address over plain http has none) or a channel, every tab has its own.
 */
const LOCK_NAME = 'llm-tulpa-live'
const channel = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel('llm-tulpa-live')
const canShare = channel != null && typeof navigator !== 'undefined' && navigator.locks != null

type ChannelMessage =
    /** An event as the stream delivered it */
    | { kind: 'event'; data: string }
    /** A tab that started listening asks whether the stream is open */
    | { kind: 'hello' }

/** Whether this tab holds the stream and it is open right now */
let streamOpen = false

/** Hands one parsed SSE message to every listener; a malformed one is ignored, a failing listener doesn't stop the rest */
const dispatch = (data: string) => {
    let event: ServerEvent
    try {
        event = JSON.parse(data) as ServerEvent
    } catch {
        return
    }

    debugLog('sse', 'event', event.type, 'chat_id' in event ? event.chat_id : '', 'reason' in event ? event.reason : '')
    for (const listener of listeners) {
        try {
            listener(event)
        } catch (error) {
            console.error('server event listener failed', error)
        }
    }
}

/** Reads one open `text/event-stream` response until it ends, calling `dispatch` per message */
const readStream = async (response: Response) => {
    const reader = response.body?.getReader()
    if (!reader) return

    const decoder = new TextDecoder()
    let buffer = ''
    for (; ;) {
        const {done, value} = await reader.read()
        // Every read, a keep-alive comment included: how long a tab goes without hearing anything shows here
        debugLog('sse', done ? 'stream ended by the server' : `chunk of ${value.length} bytes`)
        if (done) return

        buffer += decoder.decode(value, {stream: true})
        let boundary = buffer.indexOf('\n\n')
        while (boundary !== -1) {
            const data = buffer
                .slice(0, boundary)
                .split('\n')
                .filter((line) => line.startsWith('data:'))
                .map((line) => line.slice(5).trimStart())
                .join('\n')
            buffer = buffer.slice(boundary + 2)
            if (data) relay(data)
            boundary = buffer.indexOf('\n\n')
        }
    }
}

let probed = false

/**
 * Debug only, once per page, after the stream first fails: three short requests that tell a network that is down
 * from a browser that refuses the stream's headers or its address (a content blocker can do either). Each says whether
 * it got an answer.
 */
const probeBackend = async (token: string | null) => {
    if (probed || !isDebug()) return
    probed = true
    const attempt = async (label: string, path: string, headers?: Record<string, string>) => {
        try {
            const response = await fetch(`${BACKEND_URL}${path}`, {headers})
            debugLog('sse', 'probe', label, 'answered', response.status)
        } catch (error) {
            debugLog('sse', 'probe', label, 'failed', error)
        }
    }
    await attempt('status, no headers', '/api/setup/status')
    await attempt('status, same headers as the stream', '/api/setup/status', {Accept: 'text/event-stream', ...(token ? {Authorization: `Bearer ${token}`} : {})})
    await attempt('the stream address, no headers', '/api/live')
}

/**
 * Keeps `GET /api/live` open until aborted, reconnecting when it drops. Uses fetch rather than
 * EventSource because the route needs the session token and EventSource can't send a header
 */
// TODO: nothing follows runs while this stream is down or refused (a content blocker, a proxy that buffers): a page then
// keeps showing a run that has ended until it is reloaded or its tab comes back. If that bites, poll `GET /api/agent/runs` and
// `GET /api/agent/turn` every few seconds while the stream is down; left out on purpose while it is rare.
const connect = async (signal: AbortSignal) => {
    while (!signal.aborted) {
        try {
            const token = getToken()
            debugLog('sse', 'connecting', `${BACKEND_URL}/api/live`, 'from', window.location.origin)
            const response = await fetch(`${BACKEND_URL}/api/live`, {
                headers: {Accept: 'text/event-stream', ...(token ? {Authorization: `Bearer ${token}`} : {})},
                signal,
            })
            debugLog('sse', 'response', response.status)
            if (response.ok) {
                streamOpen = true
                // Told to every listener, in every tab: what happened while the stream was down (or before it was
                // first open) was never delivered, and each one reads what it needs again
                relay(JSON.stringify({type: 'stream_open'}))
                await readStream(response)
            }
        } catch (error) {
            if (signal.aborted) return
            debugLog('sse', 'connection failed', error instanceof Error ? `${error.name}: ${error.message}` : error)
            void probeBackend(getToken())
        } finally {
            streamOpen = false
        }
        debugLog('sse', `reconnecting in ${RECONNECT_MS} ms`)
        await new Promise((resolve) => setTimeout(resolve, RECONNECT_MS))
    }
}

/** Dispatches an event the stream delivered here, and passes it on to the other tabs */
const relay = (data: string) => {
    dispatch(data)
    if (canShare) channel?.postMessage({kind: 'event', data} satisfies ChannelMessage)
}

channel?.addEventListener('message', (message: MessageEvent<ChannelMessage>) => {
    if (!canShare || !controller) return
    const said = message.data
    if (said.kind === 'event') {
        dispatch(said.data)
    } else if (said.kind === 'hello' && streamOpen) {
        // The newcomer missed nothing it could have heard yet, but it is how its listeners learn the stream is up
        channel?.postMessage({kind: 'event', data: JSON.stringify({type: 'stream_open'})} satisfies ChannelMessage)
    }
})

/** Connects once this tab is the one to hold the stream; until then it hears the events from the tab that does */
const start = (signal: AbortSignal) => {
    if (!canShare) {
        void connect(signal)
        return
    }
    debugLog('sse', 'waiting for the stream lock; until then events come from another tab')
    channel?.postMessage({kind: 'hello'} satisfies ChannelMessage)
    navigator.locks
        .request(LOCK_NAME, {signal}, async () => {
            debugLog('sse', 'this tab holds the stream')
            await connect(signal)
        })
        .catch(() => {
            // Aborted while waiting for the lock: the tab stopped listening before its turn came
        })
}

/** Adds a listener, opening the shared connection for the first one; returns the remover */
const subscribe = (listener: Listener): (() => void) => {
    if (closeTimer) {
        clearTimeout(closeTimer)
        closeTimer = null
    }
    listeners.add(listener)
    if (!controller) {
        controller = new AbortController()
        start(controller.signal)
    }

    return () => {
        listeners.delete(listener)
        if (listeners.size > 0 || closeTimer) return

        closeTimer = setTimeout(() => {
            closeTimer = null
            if (listeners.size > 0) return
            controller?.abort()
            controller = null
        }, CLOSE_GRACE_MS)
    }
}

/** Calls onEvent for every event the backend pushes, for as long as the component is mounted */
export const useServerEvents = (onEvent: Listener) => {
    const onEventRef = useRef(onEvent)
    onEventRef.current = onEvent

    useEffect(() => subscribe((event) => onEventRef.current(event)), [])
}

/** useServerEvents for a single event type, with the event narrowed to its shape */
export const useServerEvent = <T extends ServerEvent['type']>(
    type: T,
    onEvent: (event: Extract<ServerEvent, { type: T }>) => void,
) => {
    const onEventRef = useRef(onEvent)
    onEventRef.current = onEvent

    useServerEvents((event) => {
        if (event.type === type) onEventRef.current(event as Extract<ServerEvent, { type: T }>)
    })
}
