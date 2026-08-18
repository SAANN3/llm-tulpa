import { forwardRef } from 'react'

import type { TextFieldProps, ThemedProps } from './types'

export const TextField = forwardRef<HTMLTextAreaElement, ThemedProps<TextFieldProps>>(function TextField(
  { style, className, variant = 'secondary', text, onChanged, placeholder, disabled, onHovered, onKeyDown },
  ref,
) {
  return (
    <textarea
      ref={ref}
      style={style}
      className={className}
      data-variant={variant}
      value={text}
      placeholder={placeholder}
      disabled={disabled}
      onChange={(e) => onChanged(e.target.value)}
      onMouseEnter={() => onHovered?.(true)}
      onMouseLeave={() => onHovered?.(false)}
      onKeyDown={onKeyDown}
    />
  )
})
