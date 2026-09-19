import { useEffect, useRef } from 'react'

import { BACKEND_URL } from '../config'

/** Everything `GET /api/events` can send — mirrors the backend's `ServerEvent`. Adding an
 * event type is one more member here; nothing else in this file changes. */
export type ServerEvent = { type: 'job_finished'; chat_id: number; job_id: number }

type Listener = (event: ServerEvent) => void

/** How long the connection stays open after its last subscriber unmounts. Long enough
 * that React's dev-mode remount, or one page unmounting as the next mounts, reuses the
 * open connection instead of closing and reopening it. */
const CLOSE_GRACE_MS = 1000

// One connection for the whole page, however many components subscribe: a browser only
// allows a handful of simultaneous connections per origin, and every subscriber would
// receive the same events anyway. Module state, deliberately — nothing here belongs to
// any one component.
const listeners = new Set<Listener>()
let source: EventSource | null = null
let closeTimer: ReturnType<typeof setTimeout> | null = null

function open() {
  if (source) return

  source = new EventSource(`${BACKEND_URL}/api/events`)
  // Every event arrives as an unnamed message carrying its own `type`, so this one
  // handler covers every event the backend will ever send.
  source.onmessage = (message: MessageEvent<string>) => {
    let event: ServerEvent
    try {
      event = JSON.parse(message.data) as ServerEvent
    } catch {
      // A malformed event is ignored — same reasoning as a missed one, see below.
      return
    }

    for (const listener of listeners) {
      // One subscriber failing must not stop the others hearing the event.
      try {
        listener(event)
      } catch (error) {
        console.error('server event listener failed', error)
      }
    }
  }
}

/** Adds `listener` and opens the connection if this is the first one — nothing connects
 * until something asks for events. Returns the function that removes it; the connection
 * closes `CLOSE_GRACE_MS` after the last one is gone. */
function subscribe(listener: Listener): () => void {
  if (closeTimer) {
    clearTimeout(closeTimer)
    closeTimer = null
  }
  listeners.add(listener)
  open()

  return () => {
    listeners.delete(listener)
    if (listeners.size > 0 || closeTimer) return

    closeTimer = setTimeout(() => {
      closeTimer = null
      if (listeners.size > 0) return
      source?.close()
      source = null
    }, CLOSE_GRACE_MS)
  }
}

/**
 * Calls `onEvent` for every event the backend pushes over its Server-Sent Events stream
 * (`GET /api/events`), for as long as the calling component is mounted — all of them
 * share one connection (see above). The browser reconnects by itself if it drops;
 * events sent while it was down are not replayed, which is fine: each one is only a
 * hint that something changed (see the backend's `ServerEvent`), so a missed one costs
 * promptness, never correctness. `onEvent` is read through a ref, so passing a fresh
 * closure each render is fine and doesn't resubscribe.
 */
export function useServerEvents(onEvent: Listener) {
  const onEventRef = useRef(onEvent)
  onEventRef.current = onEvent

  useEffect(() => subscribe((event) => onEventRef.current(event)), [])
}

/** `useServerEvents` for one event type — `onEvent` only runs for events of that
 * `type`, already narrowed to its shape. */
export function useServerEvent<T extends ServerEvent['type']>(
  type: T,
  onEvent: (event: Extract<ServerEvent, { type: T }>) => void,
) {
  const onEventRef = useRef(onEvent)
  onEventRef.current = onEvent

  useServerEvents((event) => {
    if (event.type === type) onEventRef.current(event as Extract<ServerEvent, { type: T }>)
  })
}
