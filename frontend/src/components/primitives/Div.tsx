import { forwardRef } from 'react'

import type { DivProps, ThemedProps } from './types'

export const Div = forwardRef<HTMLDivElement, ThemedProps<DivProps>>(function Div(
  { style, className, variant, children, onClick, onContextMenu, onMouseDown, onHover, onScroll },
  ref,
) {
  return (
    <div
      ref={ref}
      style={style}
      className={className}
      data-variant={variant}
      onClick={onClick}
      onContextMenu={onContextMenu}
      onMouseDown={onMouseDown}
      onMouseEnter={() => onHover?.(true)}
      onMouseLeave={() => onHover?.(false)}
      onScroll={onScroll}
    >
      {children}
    </div>
  )
})
