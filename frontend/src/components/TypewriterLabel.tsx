import { useEffect, useState } from 'react'

import type { ThemedProps } from './primitives/types'

export interface TypewriterLabelProps {
  text: string
  /** How long each revealed character sits before the next one appears, in ms. Defaults to `DEFAULT_CHAR_INTERVAL_MS` — quick enough to read as "typed fast," not a slow reveal. Short text may want a larger value so it doesn't finish instantly. */
  charIntervalMs?: number
}

const DEFAULT_CHAR_INTERVAL_MS = 18

export function TypewriterLabel({ style, className, variant = 'secondary', text, charIntervalMs = DEFAULT_CHAR_INTERVAL_MS }: ThemedProps<TypewriterLabelProps>) {
  const [visibleCount, setVisibleCount] = useState(0)

  useEffect(() => {
    setVisibleCount(0)
    if (!text) return

    const id = setInterval(() => {
      setVisibleCount((count) => {
        if (count >= text.length) {
          clearInterval(id)
          return count
        }
        return count + 1
      })
    }, charIntervalMs)

    return () => clearInterval(id)
  }, [text, charIntervalMs])

  return (
    <span style={style} className={className} data-variant={variant}>
      {text.slice(0, visibleCount)}
      <span className="caret">_</span>
    </span>
  )
}
