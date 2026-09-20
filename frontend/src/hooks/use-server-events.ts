import {useEffect, useRef} from 'react'
import {BACKEND_URL} from '../config'
import {getToken} from '../utils/auth-token'

export type ServerEvent = { type: 'job_finished'; chat_id: number; job_id: number }

type Listener = (event: ServerEvent) => void

/** How long the connection stays open after its last subscriber unmounts, so a remount reuses it */
const CLOSE_GRACE_MS = 1000

/** Delay before reconnecting after the stream drops */
const RECONNECT_MS = 3000

const listeners = new Set<Listener>()
let controller: AbortController | null = null
let closeTimer: ReturnType<typeof setTimeout> | null = null

/** Hands one parsed SSE message to every listener; a malformed one is ignored, a failing listener doesn't stop the rest */
const dispatch = (data: string) => {
    let event: ServerEvent
    try {
        event = JSON.parse(data) as ServerEvent
    } catch {
        return
    }

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
            if (data) dispatch(data)
            boundary = buffer.indexOf('\n\n')
        }
    }
}

/**
 * Keeps `GET /api/events` open until aborted, reconnecting when it drops. Uses fetch rather than
 * EventSource because the route needs the session token and EventSource can't send a header
 */
const connect = async (signal: AbortSignal) => {
    while (!signal.aborted) {
        try {
            const token = getToken()
            const response = await fetch(`${BACKEND_URL}/api/events`, {
                headers: {Accept: 'text/event-stream', ...(token ? {Authorization: `Bearer ${token}`} : {})},
                signal,
            })
            if (response.ok) await readStream(response)
        } catch {
            if (signal.aborted) return
        }
        await new Promise((resolve) => setTimeout(resolve, RECONNECT_MS))
    }
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
        void connect(controller.signal)
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
