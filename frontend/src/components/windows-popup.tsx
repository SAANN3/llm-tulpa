import type {CSSProperties, MouseEvent as ReactMouseEvent, ReactNode} from 'react'
import {useRef, useState} from 'react'
import {Close, Download} from 'pixelarticons/react'
import {Button, Div, Label} from './primitives'

export interface WindowsPopupProps {
    open: boolean
    onClose: () => void
    title: string
    children: ReactNode
    onDownload?: () => void
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
const HANDLE = 10

const clamp = (value: number, min: number, max: number): number => Math.min(Math.max(value, min), max);

/** Covers the viewport during a drag/resize so mouse events aren't lost to an iframe */
const coverViewportDuringDrag = (cursor: string): () => void => {
    const overlay = document.createElement('div')
    overlay.style.position = 'fixed'
    overlay.style.inset = '0'
    overlay.style.zIndex = '99999'
    overlay.style.cursor = cursor
    document.body.appendChild(overlay)
    return () => overlay.remove()
};

/** A themed, draggable, resizable window over freeform content, retro-OS style */
export const WindowsPopup = ({open, onClose, title, children, onDownload, defaultSize}: WindowsPopupProps) => {
    const [position, setPosition] = useState<{ x: number; y: number } | null>(null)
    const [size, setSize] = useState<{ width: number; height: number } | null>(defaultSize ?? null)
    const [dragging, setDragging] = useState(false)
    const windowRef = useRef<HTMLDivElement>(null)
    const contentRef = useRef<HTMLDivElement>(null)

    const startDrag = (e: ReactMouseEvent) => {
        const rect = windowRef.current?.getBoundingClientRect()
        if (!rect) return
        e.preventDefault()

        const startX = e.clientX
        const startY = e.clientY
        const originX = rect.left
        const originY = rect.top
        setDragging(true)
        const removeOverlay = coverViewportDuringDrag('grabbing')

        const handleMove = (moveEvent: MouseEvent) => {
            setPosition({x: originX + (moveEvent.clientX - startX), y: originY + (moveEvent.clientY - startY)})
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
        e.stopPropagation()

        const anchorX = windowRect.left
        const anchorY = windowRect.top
        const startWidth = contentRect.width
        const startHeight = contentRect.height
        const startX = e.clientX
        const startY = e.clientY
        const maxWidth = window.innerWidth * 0.92
        const maxHeight = window.innerHeight * 0.85

        setPosition({x: anchorX, y: anchorY})
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

            setSize({width, height})
            if (edge.includes('w') || edge.includes('n')) setPosition({x, y})
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
        ? {left: position.x, top: position.y}
        : {left: '50%', top: '50%', transform: 'translate(-50%, -50%)'}

    const handleStyle = (edge: ResizeEdge): CSSProperties => {
        const half = -HANDLE / 2
        const base: CSSProperties = {position: 'absolute', cursor: EDGE_CURSORS[edge], zIndex: 2}
        if (edge === 'n') return {...base, top: half, left: HANDLE, right: HANDLE, height: HANDLE}
        if (edge === 's') return {...base, bottom: half, left: HANDLE, right: HANDLE, height: HANDLE}
        if (edge === 'w') return {...base, left: half, top: HANDLE, bottom: HANDLE, width: HANDLE}
        if (edge === 'e') return {...base, right: half, top: HANDLE, bottom: HANDLE, width: HANDLE}
        if (edge === 'nw') return {...base, top: half, left: half, width: HANDLE * 2, height: HANDLE * 2}
        if (edge === 'ne') return {...base, top: half, right: half, width: HANDLE * 2, height: HANDLE * 2}
        if (edge === 'sw') return {...base, bottom: half, left: half, width: HANDLE * 2, height: HANDLE * 2}
        return {...base, bottom: half, right: half, width: HANDLE * 2, height: HANDLE * 2}
    }

    return (
        <Div
            ref={windowRef}
            style={{
                position: 'fixed',
                ...placement,
                width: size ? size.width : undefined,
                zIndex: 1000,
                boxShadow: '4px 4px 0 0 rgba(0, 0, 0, 0.4)',
            }}
        >
            <Div
                variant="tertiary"
                onMouseDown={startDrag}
                style={{
                    position: 'relative',
                    zIndex: 3,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                    gap: 12,
                    padding: '4px 4px 4px 10px',
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
                        minWidth: 0,
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                    }}
                />
                <Div onMouseDown={(e) => e.stopPropagation()} style={{display: 'flex', gap: 4, flexShrink: 0}}>
                    {onDownload ? (
                        <Button
                            onClicked={onDownload}
                            style={{
                                width: 20,
                                height: 20,
                                padding: 0,
                                borderRadius: 0,
                                display: 'flex',
                                alignItems: 'center',
                                justifyContent: 'center'
                            }}
                        >
                            <Download width={13} height={13}/>
                        </Button>
                    ) : null}
                    <Button
                        onClicked={onClose}
                        style={{
                            width: 20,
                            height: 20,
                            padding: 0,
                            borderRadius: 0,
                            display: 'flex',
                            alignItems: 'center',
                            justifyContent: 'center'
                        }}
                    >
                        <Close width={14} height={14}/>
                    </Button>
                </Div>
            </Div>
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
                <div key={edge} onMouseDown={startResize(edge)} style={handleStyle(edge)}/>
            ))}
        </Div>
    )
};
