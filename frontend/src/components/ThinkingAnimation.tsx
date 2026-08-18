import type { ThemedProps } from './primitives/types'

export interface ThinkingAnimationProps {
  isPlaying: boolean
}

/** Three dots that pulse in sequence while `isPlaying`, and settle fully lit and still once it stops — a typing-indicator-style "thinking → done" cue. Colors come from `currentColor`, same as every themed primitive, so no variant wiring is needed here beyond `data-variant` itself. */
export function ThinkingAnimation({ style, className, variant = 'secondary', isPlaying }: ThemedProps<ThinkingAnimationProps>) {
  return (
    <div style={style} className={className} data-variant={variant} data-thinking data-playing={isPlaying}>
      <span data-thinking-dot />
      <span data-thinking-dot />
      <span data-thinking-dot />
    </div>
  )
}
