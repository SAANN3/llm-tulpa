import type { MouseEvent as ReactMouseEvent, ReactNode } from 'react'
import { useRef, useState } from 'react'

import { Button, Div, Label } from './primitives'

export interface WindowsPopupProps {
  open: boolean
  onClose: () => void
  title: string
  children: ReactNode
}

/** A themed, draggable little window — title bar (grab to move, × to close) over
 * freeform content, retro-OS style. Starts centered on open; once dragged, stays
 * wherever it's put until closed (position isn't remembered across opens). Unlike
 * `Popup`, doesn't close on an outside click — that would fight dragging past its own
 * bounds — only the × does. */
export function WindowsPopup({ open, onClose, title, children }: WindowsPopupProps) {
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null)
  const [dragging, setDragging] = useState(false)
  const windowRef = useRef<HTMLDivElement>(null)

  const startDrag = (e: ReactMouseEvent) => {
    const rect = windowRef.current?.getBoundingClientRect()
    if (!rect) return
    e.preventDefault() // otherwise a fast drag selects the title text/page underneath

    const startX = e.clientX
    const startY = e.clientY
    const originX = rect.left
    const originY = rect.top
    setDragging(true)

    const handleMove = (moveEvent: MouseEvent) => {
      setPosition({ x: originX + (moveEvent.clientX - startX), y: originY + (moveEvent.clientY - startY) })
    }
    const handleUp = () => {
      setDragging(false)
      document.removeEventListener('mousemove', handleMove)
      document.removeEventListener('mouseup', handleUp)
    }
    document.addEventListener('mousemove', handleMove)
    document.addEventListener('mouseup', handleUp)
  }

  if (!open) return null

  const placement = position
    ? { left: position.x, top: position.y }
    : { left: '50%', top: '50%', transform: 'translate(-50%, -50%)' }

  return (
    <Div
      ref={windowRef}
      style={{
        position: 'fixed',
        ...placement,
        zIndex: 1000,
        boxShadow: '0 4px 16px rgba(0, 0, 0, 0.3)',
      }}
    >
      <Div
        variant="tertiary"
        onMouseDown={startDrag}
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          gap: 12,
          padding: '4px 4px 4px 10px',
          // Themed `Div`s get an 8px radius from `variants.css` by default — overridden
          // back to square here, deliberately, for the retro-window look this wants.
          borderRadius: 0,
          cursor: dragging ? 'grabbing' : 'grab',
          userSelect: 'none',
        }}
      >
        <Label text={title} style={{ fontSize: 12, fontWeight: 600 }} />
        <Button
          text="×"
          onClicked={onClose}
          style={{ width: 20, height: 20, padding: 0, borderRadius: 4, fontSize: 13, lineHeight: 1, flexShrink: 0 }}
        />
      </Div>
      {/* The frame around the content, deliberately separate from the title bar's own
          color above — the Windows-95-ish look this is going for is a bar in the accent
          color sitting flush on top of a plainer-bordered pane, not one uniform box. */}
      <Div variant="secondary" style={{ border: '3px solid var(--color-border)', borderRadius: 0 }}>
        {children}
      </Div>
    </Div>
  )
}
