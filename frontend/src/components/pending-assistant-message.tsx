import { useEffect, useRef, useState } from 'react'

import '../styles/pending-assistant-message.scss'
import { Div, Label } from './primitives'
import { ThinkingAnimation } from './thinking-animation.tsx'

/** Placeholder shown where the next assistant reply will land while a turn is in flight */
export function PendingAssistantMessage() {
  const startedAtRef = useRef(Date.now())
  const [elapsedSeconds, setElapsedSeconds] = useState(0)

  useEffect(() => {
    /** Recomputes elapsed time from a fixed start so background throttling doesn't leave it stale */
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
