import type { CSSProperties, MouseEvent as ReactMouseEvent, ReactNode } from 'react'
import { useRef, useState } from 'react'

import { Button, Div, Label } from './primitives'

export interface WindowsPopupProps {
  open: boolean
  onClose: () => void
  title: string
  children: ReactNode
  /** Renders a small download icon button in the title bar, immediately left of the ×
   * close button. Omit for a popup with nothing to download. */
  onDownload?: () => void
  /** Initial content-pane size in px, for content with no natural size of its own to
   * start at (a PDF/webpage/document/table — as opposed to an image, which sizes
   * itself to its own dimensions). Omit to let the pane size to its content naturally
   * until the user first resizes it. */
  defaultSize?: { width: number; height: number }
}

type ResizeEdge = 'n' | 's' | 'e' | 'w' | 'ne' | 'nw' | 'se' | 'sw'

const EDGE_CURSORS: Record<ResizeEdge, string> = {
  n: 'ns-resize',
  s: 'ns-resize',
  e: 'ew-resize',
  w: 'ew-resize',
  ne: 'nesw-resize',
  sw: 'nesw-resize',
  nw: 'nwse-resize',
  se: 'nwse-resize',
}

const MIN_WIDTH = 240
const MIN_HEIGHT = 120
/** Thickness (px) of each edge/corner's grab zone — straddles the border (half
 * outside, half in) so it's easy to actually land the cursor on. */
const HANDLE = 10

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

/** Covers the whole viewport for the duration of a drag/resize, so every `mousemove`/
 * `mouseup` lands on the outer document no matter what the cursor physically passes
 * over — in particular an `<iframe>` (a PDF/HTML preview), which has its own separate
 * document and simply never bubbles its events up to this one. Without this, a
 * `mouseup` that happens to land inside an iframe is invisible to the `document`-level
 * listener below: the drag never formally ends, the listeners stay attached, and the
 * next `mousemove` anywhere (even after the button's been released) keeps moving/
 * resizing the window as if the button were still down — exactly what an iframe
 * swallowing the real mouseup looks like. Returns a cleanup function that removes it. */
function coverViewportDuringDrag(cursor: string): () => void {
  const overlay = document.createElement('div')
  overlay.style.position = 'fixed'
  overlay.style.inset = '0'
  overlay.style.zIndex = '99999'
  overlay.style.cursor = cursor
  document.body.appendChild(overlay)
  return () => overlay.remove()
}

/** A themed, draggable, resizable little window — title bar (grab to move, × to
 * close) over freeform content, retro-OS style. Starts centered on open; once dragged
 * or resized, stays wherever/whatever size it's left at until closed (neither is
 * remembered across opens). Unlike `Popup`, doesn't close on an outside click — that
 * would fight dragging/resizing past its own bounds — only the × does.
 *
 * Resizable from any of its 8 edges/corners, same as a normal desktop window — hand-
 * rolled (not CSS `resize`, which only ever offers the bottom-right corner) the same
 * way drag-to-move already is. Resizing from a `w`/`n` edge keeps the *opposite* edge
 * fixed and grows/shrinks toward it, rather than resizing around the window's center —
 * that takes pinning the window to a real measured pixel position the moment a resize
 * (or a drag) first happens, since before that it's simply centered via CSS
 * (`left/top: 50%` + a `translate(-50%, -50%)`), and translating by a *percentage*
 * recomputes against the box's current size on every resize, which is exactly what
 * "resizes around the center" looks like.
 */
export function WindowsPopup({ open, onClose, title, children, onDownload, defaultSize }: WindowsPopupProps) {
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null)
  const [size, setSize] = useState<{ width: number; height: number } | null>(defaultSize ?? null)
  const [dragging, setDragging] = useState(false)
  const windowRef = useRef<HTMLDivElement>(null)
  const contentRef = useRef<HTMLDivElement>(null)

  const startDrag = (e: ReactMouseEvent) => {
    const rect = windowRef.current?.getBoundingClientRect()
    if (!rect) return
    e.preventDefault() // otherwise a fast drag selects the title text/page underneath

    const startX = e.clientX
    const startY = e.clientY
    const originX = rect.left
    const originY = rect.top
    setDragging(true)
    const removeOverlay = coverViewportDuringDrag('grabbing')

    const handleMove = (moveEvent: MouseEvent) => {
      setPosition({ x: originX + (moveEvent.clientX - startX), y: originY + (moveEvent.clientY - startY) })
    }
    const handleUp = () => {
      setDragging(false)
      removeOverlay()
      document.removeEventListener('mousemove', handleMove)
      document.removeEventListener('mouseup', handleUp)
    }
    document.addEventListener('mousemove', handleMove)
    document.addEventListener('mouseup', handleUp)
  }

  const startResize = (edge: ResizeEdge) => (e: ReactMouseEvent) => {
    const windowRect = windowRef.current?.getBoundingClientRect()
    const contentRect = contentRef.current?.getBoundingClientRect()
    if (!windowRect || !contentRect) return
    e.preventDefault()
    e.stopPropagation() // don't also let this bubble into the title bar's own drag

    // Pins the window at its actual current on-screen position, in real pixels — see
    // this component's own doc comment for why that matters before resizing starts.
    const anchorX = windowRect.left
    const anchorY = windowRect.top
    const startWidth = contentRect.width
    const startHeight = contentRect.height
    const startX = e.clientX
    const startY = e.clientY
    const maxWidth = window.innerWidth * 0.92
    const maxHeight = window.innerHeight * 0.85

    setPosition({ x: anchorX, y: anchorY })
    const removeOverlay = coverViewportDuringDrag(EDGE_CURSORS[edge])

    const handleMove = (moveEvent: MouseEvent) => {
      const dx = moveEvent.clientX - startX
      const dy = moveEvent.clientY - startY

      let width = startWidth
      let height = startHeight
      let x = anchorX
      let y = anchorY

      if (edge.includes('e')) width = clamp(startWidth + dx, MIN_WIDTH, maxWidth)
      if (edge.includes('w')) {
        width = clamp(startWidth - dx, MIN_WIDTH, maxWidth)
        x = anchorX + (startWidth - width)
      }
      if (edge.includes('s')) height = clamp(startHeight + dy, MIN_HEIGHT, maxHeight)
      if (edge.includes('n')) {
        height = clamp(startHeight - dy, MIN_HEIGHT, maxHeight)
        y = anchorY + (startHeight - height)
      }

      setSize({ width, height })
      if (edge.includes('w') || edge.includes('n')) setPosition({ x, y })
    }
    const handleUp = () => {
      removeOverlay()
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

  const handleStyle = (edge: ResizeEdge): CSSProperties => {
    const half = -HANDLE / 2
    const base: CSSProperties = { position: 'absolute', cursor: EDGE_CURSORS[edge], zIndex: 2 }
    if (edge === 'n') return { ...base, top: half, left: HANDLE, right: HANDLE, height: HANDLE }
    if (edge === 's') return { ...base, bottom: half, left: HANDLE, right: HANDLE, height: HANDLE }
    if (edge === 'w') return { ...base, left: half, top: HANDLE, bottom: HANDLE, width: HANDLE }
    if (edge === 'e') return { ...base, right: half, top: HANDLE, bottom: HANDLE, width: HANDLE }
    if (edge === 'nw') return { ...base, top: half, left: half, width: HANDLE * 2, height: HANDLE * 2 }
    if (edge === 'ne') return { ...base, top: half, right: half, width: HANDLE * 2, height: HANDLE * 2 }
    if (edge === 'sw') return { ...base, bottom: half, left: half, width: HANDLE * 2, height: HANDLE * 2 }
    return { ...base, bottom: half, right: half, width: HANDLE * 2, height: HANDLE * 2 } // 'se'
  }

  return (
    <Div
      ref={windowRef}
      style={{
        position: 'fixed',
        ...placement,
        // Pinned to the content pane's own tracked width so a long title can never
        // force the window (title bar included) wider than the content pane sitting
        // right below it — without this, the outer box just shrink-wraps to
        // whichever of its two children is widest, and once the content pane has an
        // explicit *resized* width narrower than the title needs, the title bar (and
        // the window itself) would stick out past it — the sliver of window to the
        // right of the now-narrower content pane belongs to neither child, so nothing
        // paints a background there; it just shows whatever's behind the popup. The
        // title truncates with an ellipsis instead (see the `Label` below).
        // `undefined` pre-resize is fine: the one popup that can start without a real
        // tracked width (an image's own, via no `defaultSize`) also always has a
        // short, fixed title ("Preview", never the file name) that could never
        // trigger this in the first place.
        width: size ? size.width : undefined,
        zIndex: 1000,
        boxShadow: '0 4px 16px rgba(0, 0, 0, 0.3)',
      }}
    >
      <Div
        variant="tertiary"
        onMouseDown={startDrag}
        style={{
          position: 'relative',
          // Above the corner resize handles (zIndex 2) — the `ne`/`nw` corners'
          // grab zones necessarily overlap the title bar's own corner, where the
          // download/× buttons live, and a button that's supposed to be right there
          // has to actually win that overlap or it stops being clickable.
          zIndex: 3,
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
        <Label
          text={title}
          style={{
            fontSize: 12,
            fontWeight: 600,
            flex: 1,
            // A flex item's own default `min-width` is its content's full intrinsic
            // size, not 0 — without overriding that, a long title still refuses to
            // shrink below its own natural width no matter what `overflow`/`white-
            // space` say, the classic flexbox gotcha that makes ellipsis truncation
            // silently do nothing inside a flex row.
            minWidth: 0,
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            whiteSpace: 'nowrap',
          }}
        />
        {/* Stops the mousedown here — otherwise it bubbles up into the title bar's own
            `onMouseDown={startDrag}` (a real, if usually harmless, side effect on its
            own: a drag "starts" and immediately ends on the same click), which now
            actually breaks the button: `startDrag` inserts the full-viewport drag
            overlay (see `coverViewportDuringDrag`) the instant it runs, so by the time
            `mouseup` fires, the overlay — not the button — is the topmost element
            there, and the browser never synthesizes a `click` on a mousedown/mouseup
            pair that landed on two different elements. The button still visually
            reacts to the press (`:active`), just never actually fires. */}
        <Div onMouseDown={(e) => e.stopPropagation()} style={{ display: 'flex', gap: 4, flexShrink: 0 }}>
          {onDownload ? (
            <Button
              onClicked={onDownload}
              style={{ width: 20, height: 20, padding: 0, borderRadius: 4, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
            >
              <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                <path d="M12 3v13m0 0l-5-5m5 5l5-5M4 21h16" />
              </svg>
            </Button>
          ) : null}
          <Button
            text="×"
            onClicked={onClose}
            style={{ width: 20, height: 20, padding: 0, borderRadius: 4, fontSize: 13, lineHeight: 1 }}
          />
        </Div>
      </Div>
      {/* The frame around the content, deliberately separate from the title bar's own
          color above — the Windows-95-ish look this is going for is a bar in the accent
          color sitting flush on top of a plainer-bordered pane, not one uniform box. */}
      <Div
        ref={contentRef}
        variant="secondary"
        style={{
          border: '3px solid var(--color-border)',
          borderRadius: 0,
          overflow: 'auto',
          boxSizing: 'border-box',
          width: size ? size.width : undefined,
          height: size ? size.height : undefined,
          minWidth: MIN_WIDTH,
          minHeight: MIN_HEIGHT,
          maxWidth: size ? undefined : '92vw',
          maxHeight: size ? undefined : '85vh',
        }}
      >
        {children}
      </Div>
      {(['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as const).map((edge) => (
        <div key={edge} onMouseDown={startResize(edge)} style={handleStyle(edge)} />
      ))}
    </Div>
  )
}
