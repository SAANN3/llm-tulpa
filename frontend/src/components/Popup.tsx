import { useEffect, useRef } from 'react'
import type { ReactNode } from 'react'

import { Div } from './primitives'

export interface PopupProps {
  open: boolean
  onClose: () => void
  /** Viewport coordinates (top-left corner) to anchor the popup at. Ignored when `centered` is true. */
  position?: { x: number; y: number }
  /** Renders fixed at the viewport's center instead of at `position` — for dialogs (confirm/rename) rather than a menu anchored to whatever triggered it. */
  centered?: boolean
  children: ReactNode
}

/**
 * A themed, positioned overlay that closes itself on any click outside its own bounds —
 * the shared mechanics behind any right-click/three-dot style menu in the app, and also
 * behind centered confirm/edit dialogs (`centered`). Callers own what's inside
 * (`children`) and when/where it opens (`open`/`position`/`centered`); this only handles
 * positioning, sitting above everything else, and the outside-click-to-close behavior.
 * Renders nothing while `open` is false.
 */
export function Popup({ open, onClose, position, centered, children }: PopupProps) {
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return

    const handlePointerDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        onClose()
      }
    }

    document.addEventListener('mousedown', handlePointerDown)
    return () => document.removeEventListener('mousedown', handlePointerDown)
  }, [open, onClose])

  if (!open) return null

  const placement = centered
    ? { left: '50%', top: '50%', transform: 'translate(-50%, -50%)' }
    : { left: position?.x ?? 0, top: position?.y ?? 0 }

  return (
    <Div
      ref={ref}
      variant="secondary"
      className="vbox"
      style={{
        position: 'fixed',
        ...placement,
        zIndex: 1000,
        border: '1px solid currentColor',
        borderRadius: 6,
        overflow: 'hidden',
        boxShadow: '0 4px 16px rgba(0, 0, 0, 0.3)',
      }}
    >
      {children}
    </Div>
  )
}
