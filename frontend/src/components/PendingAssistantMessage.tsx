import { useEffect, useRef, useState } from 'react'

import '../styles/PendingAssistantMessage.scss'
import { Div, Label } from './primitives'
import { ThinkingAnimation } from './ThinkingAnimation'

/** Placeholder shown where the next assistant reply will land while a turn is in flight — a pulsing dots animation plus a live elapsed-seconds counter, replaced by the real `ChatMessage` once the reply arrives. */
export function PendingAssistantMessage() {
  const startedAtRef = useRef(Date.now())
  const [elapsedSeconds, setElapsedSeconds] = useState(0)

  useEffect(() => {
    // Computed from a fixed start time on every tick, not incremented — a background
    // tab's timers get throttled by the browser, so an incrementing counter falls
    // behind real elapsed time and never catches up, making a turn that's still
    // running look like it stalled the moment you tab back in. Deriving from
    // `Date.now()` self-corrects on whatever tick actually fires. The `visibilitychange`
    // listener forces one of those right away on refocus, instead of waiting for the
    // next throttled interval tick to reflect the jump.
    const tick = () => setElapsedSeconds(Math.floor((Date.now() - startedAtRef.current) / 1000))
    const id = setInterval(tick, 1000)
    document.addEventListener('visibilitychange', tick)
    return () => {
      clearInterval(id)
      document.removeEventListener('visibilitychange', tick)
    }
  }, [])

  return (
    <Div className="vbox pending-message">
      <ThinkingAnimation isPlaying />
      <Label className="pending-message__label" text={`Thinking... (${elapsedSeconds}s)`} />
    </Div>
  )
}
