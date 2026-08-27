import type { LabelProps, ThemedProps } from './types'

export function Label({ style, className, variant, text }: ThemedProps<LabelProps>) {
  return (
    <span style={style} className={className} data-variant={variant} data-label>
      {text}
    </span>
  )
}
