import { useEffect, useState } from 'react'

import { Div, Label } from './primitives'
import { ThinkingAnimation } from './ThinkingAnimation'

/** Placeholder shown where the next assistant reply will land while a turn is in flight — a pulsing dots animation plus a live elapsed-seconds counter, replaced by the real `ChatMessage` once the reply arrives. */
export function PendingAssistantMessage() {
  const [elapsedSeconds, setElapsedSeconds] = useState(0)

  useEffect(() => {
    const id = setInterval(() => setElapsedSeconds((s) => s + 1), 1000)
    return () => clearInterval(id)
  }, [])

  return (
    <Div className="vbox" style={{ alignItems: 'flex-start', gap: 4 }}>
      <ThinkingAnimation isPlaying />
      <Label text={`Thinking... (${elapsedSeconds}s)`} style={{ fontSize: 12, opacity: 0.6 }} />
    </Div>
  )
}
