import { useEffect, useRef, useState } from 'react'

import '../styles/pending-assistant-message.scss'
import { Div, Label } from './primitives'
import { ThinkingAnimation } from './thinking-animation.tsx'

/** Placeholder shown where the next assistant reply will land while a turn is in flight */
export function PendingAssistantMessage() {
  const startedAtRef = useRef(Date.now())
  const [elapsedSeconds, setElapsedSeconds] = useState(0)

  useEffect(() => {
    // Derived from a fixed start time on every tick rather than incremented: a background
    // tab's timers are throttled, so a counter falls behind real time and a turn that is
    // still running looks stalled the moment you tab back in. `visibilitychange` forces one
    // tick on refocus instead of waiting for the next throttled interval.
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
