import type {PointerEvent as ReactPointerEvent, ReactNode} from 'react'
import {useEffect, useId, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore} from 'react'
import {Close, Collapse, Download, Expand} from 'pixelarticons/react'
import '../../../styles/windows-popup.scss'
import {Frame, type FrameAction} from '../../frame.tsx'
import {Button, Div} from '../../primitives'
import {WindowToolsContext, type WindowTools} from './window-tools.ts'

export interface WindowsPopupProps {
    open: boolean
    onClose: () => void
    title: string
    children: ReactNode
    onDownload?: () => void
    defaultSize?: { width: number; height: number }
}

type Edge = 'e' | 'w' | 's' | 'ne' | 'nw' | 'se' | 'sw'
// No top edge: the top bar is the handle for moving the window, and only its corners resize
const EDGES: Edge[] = ['e', 'w', 's', 'ne', 'nw', 'se', 'sw']

const MIN_WIDTH = 300
const MIN_HEIGHT = 180
const DEFAULT_SIZE = {width: 720, height: 520}
/** How far apart windows opened one after another are placed, so a new one doesn't hide the last exactly */
const STAGGER = 28
/** How far the pointer moves before a press on the top bar becomes a drag (a double-click must not move it) */
const DRAG_THRESHOLD = 4
/** How much one notch of Ctrl+scroll zooms */
export const WHEEL_ZOOM = 1.1
const MIN_SCALE = 0.5
const MAX_SCALE = 3

// The open windows, back to front: the last one is in front, has the keyboard, and the others are dimmed
let order: string[] = []
const listeners = new Set<() => void>()
const changed = () => listeners.forEach((listener) => listener())
const subscribe = (listener: () => void) => {
    listeners.add(listener)
    return () => listeners.delete(listener)
}
const raise = (id: string) => {
    if (order[order.length - 1] === id) return
    order = [...order.filter((other) => other !== id), id]
    changed()
}
const useOrder = () => useSyncExternalStore(subscribe, () => order)

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max)

/** Keeps the page from selecting text, and the cursor the same everywhere, while a window is moved or resized */
const holdPage = (cursor: string) => {
    document.body.classList.add('preview-window-holding')
    document.body.style.setProperty('--preview-window-cursor', cursor)
    return () => document.body.classList.remove('preview-window-holding')
}

/**
 * A file's preview in a window over the page: the app's double frame with the file's name on its top edge, moved by
 * its top bar, resized from its sides, bottom and corners, maximized with the button on its edge (or a double-click on
 * the bar). Several can be open: each opens a little below the last, the one touched comes to the front and takes the
 * keyboard, the others dim. What it shows can put tools and keys of its own in it (`WindowToolsContext`).
 */
export const WindowsPopup = ({open, onClose, title, children, onDownload, defaultSize}: WindowsPopupProps) => {
    const id = useId()
    const stack = useOrder()
    const [box, setBox] = useState<{ x: number; y: number; width: number; height: number } | null>(null)
    const [maximized, setMaximized] = useState(false)
    const [tools, setTools] = useState<WindowTools | null>(null)
    // How much scalable content is scaled (Ctrl+scroll), and the point to keep under the pointer once it is
    const [scale, setScale] = useState(1)
    const anchorRef = useRef<{ x: number; y: number; contentX: number; contentY: number; ratio: number } | null>(null)
    const windowRef = useRef<HTMLDivElement>(null)
    const contentRef = useRef<HTMLDivElement>(null)
    const toolsRef = useRef(tools)
    toolsRef.current = tools

    // The content area exists once the window is placed (it renders nothing before)
    const placed = box != null

    // Ctrl+scroll over the window zooms what it shows, never the page. A listener of its own, not React's: it has to
    // be able to cancel the browser's zoom, which a passive wheel listener (React's) can't
    useEffect(() => {
        const content = contentRef.current
        if (!open || !content) return
        const onWheel = (e: WheelEvent) => {
            if (!e.ctrlKey) return
            const current = toolsRef.current
            if (!current?.onZoom && !current?.scalable) return
            e.preventDefault()
            const rect = content.getBoundingClientRect()
            const at = {x: e.clientX - rect.left, y: e.clientY - rect.top}
            const factor = e.deltaY < 0 ? WHEEL_ZOOM : 1 / WHEEL_ZOOM
            if (current.onZoom) {
                current.onZoom(factor, at, content)
                return
            }
            setScale((now) => {
                const next = clamp(now * factor, MIN_SCALE, MAX_SCALE)
                anchorRef.current = {...at, contentX: content.scrollLeft + at.x, contentY: content.scrollTop + at.y, ratio: next / now}
                return next
            })
        }
        content.addEventListener('wheel', onWheel, {passive: false})
        return () => content.removeEventListener('wheel', onWheel)
    }, [open, placed])

    // Scrolls so the point that was under the pointer is under it again at the new scale
    useLayoutEffect(() => {
        const content = contentRef.current
        const anchor = anchorRef.current
        if (!content || !anchor) return
        anchorRef.current = null
        content.scrollLeft = anchor.contentX * anchor.ratio - anchor.x
        content.scrollTop = anchor.contentY * anchor.ratio - anchor.y
    }, [scale])

    // Joins the stack while open; placed in the middle of the window, a step down and right of each window open already
    useEffect(() => {
        if (!open) return
        const size = defaultSize ?? DEFAULT_SIZE
        const width = Math.min(size.width, window.innerWidth - 32)
        const height = Math.min(size.height, window.innerHeight - 32)
        const step = order.length * STAGGER
        setBox({
            x: clamp((window.innerWidth - width) / 2 + step, 0, window.innerWidth - width),
            y: clamp((window.innerHeight - height) / 2 + step, 0, window.innerHeight - height),
            width,
            height,
        })
        setMaximized(false)
        order = [...order, id]
        changed()
        return () => {
            order = order.filter((other) => other !== id)
            changed()
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [open, id])

    const front = stack[stack.length - 1] === id
    const depth = stack.indexOf(id)

    const startMove = (e: ReactPointerEvent<HTMLDivElement>) => {
        if (e.button !== 0 || !box) return
        e.preventDefault()
        raise(id)
        const handle = e.currentTarget
        handle.setPointerCapture(e.pointerId)
        const startX = e.clientX
        const startY = e.clientY
        let origin = {x: box.x, y: box.y}
        let moved = false
        let release: (() => void) | null = null

        const onMove = (move: PointerEvent) => {
            if (!moved && Math.hypot(move.clientX - startX, move.clientY - startY) < DRAG_THRESHOLD) return
            if (!moved) {
                release = holdPage('grabbing')
                // Taking a maximized window by its bar restores it under the pointer, at the same share of its width
                if (maximized) {
                    const rect = windowRef.current?.getBoundingClientRect()
                    const share = rect ? (startX - rect.left) / rect.width : 0.5
                    origin = {x: startX - share * box.width, y: startY - 12}
                    setMaximized(false)
                }
                moved = true
            }
            setBox((now) => now && {
                ...now,
                x: origin.x + move.clientX - startX,
                // Never above the window's top, where the bar could no longer be taken
                y: Math.max(0, origin.y + move.clientY - startY),
            })
        }
        const onUp = () => {
            release?.()
            handle.removeEventListener('pointermove', onMove)
            handle.removeEventListener('pointerup', onUp)
            handle.removeEventListener('pointercancel', onUp)
        }
        handle.addEventListener('pointermove', onMove)
        handle.addEventListener('pointerup', onUp)
        handle.addEventListener('pointercancel', onUp)
    }

    const startResize = (edge: Edge) => (e: ReactPointerEvent<HTMLDivElement>) => {
        if (e.button !== 0 || !box || maximized) return
        e.preventDefault()
        e.stopPropagation()
        raise(id)
        const handle = e.currentTarget
        handle.setPointerCapture(e.pointerId)
        const start = {...box, pointerX: e.clientX, pointerY: e.clientY}
        const release = holdPage(getComputedStyle(handle).cursor)

        const onMove = (move: PointerEvent) => {
            const dx = move.clientX - start.pointerX
            const dy = move.clientY - start.pointerY
            const next = {x: start.x, y: start.y, width: start.width, height: start.height}
            if (edge.includes('e')) next.width = clamp(start.width + dx, MIN_WIDTH, window.innerWidth - start.x)
            if (edge.includes('s')) next.height = clamp(start.height + dy, MIN_HEIGHT, window.innerHeight - start.y)
            // The left and top keep the opposite edge where it is
            if (edge.includes('w')) {
                next.width = clamp(start.width - dx, MIN_WIDTH, start.x + start.width)
                next.x = start.x + start.width - next.width
            }
            if (edge.includes('n')) {
                next.height = clamp(start.height - dy, MIN_HEIGHT, start.y + start.height)
                next.y = start.y + start.height - next.height
            }
            setBox(next)
        }
        const onUp = () => {
            release()
            handle.removeEventListener('pointermove', onMove)
            handle.removeEventListener('pointerup', onUp)
            handle.removeEventListener('pointercancel', onUp)
        }
        handle.addEventListener('pointermove', onMove)
        handle.addEventListener('pointerup', onUp)
        handle.addEventListener('pointercancel', onUp)
    }

    const toggleMaximized = () => {
        raise(id)
        setMaximized((now) => !now)
    }

    const actions: FrameAction[] = [
        ...(tools?.actions ?? []),
        ...(onDownload ? [{keys: ['d'], label: 'download', run: onDownload}] : []),
        {keys: ['m'], label: maximized ? 'restore' : 'maximize', run: toggleMaximized},
    ]

    const context = useMemo(() => ({setTools}), [])

    if (!open || !box) return null

    return (
        // A plain element: it needs the pointer events of its own that the primitives don't pass on
        <div ref={windowRef}
             className={`preview-window${front ? '' : ' preview-window--back'}${maximized ? ' preview-window--max' : ''}`}
             style={maximized ? {zIndex: 1000 + depth} : {left: box.x, top: box.y, width: box.width, height: box.height, zIndex: 1000 + depth}}
             onPointerDownCapture={() => raise(id)}>
            <Frame className="preview-window__frame" bodyClassName="preview-window__body" title={title} actions={actions}
                   onEscape={onClose} active={front}>
                <Div className="preview-window__tools">
                    {tools?.node}
                    {tools?.meta ? <span className="preview-window__meta">{tools.meta}</span> : null}
                </Div>
                <div ref={contentRef} className="preview-window__content">
                    <div className="preview-window__scaled" style={tools?.scalable && scale !== 1 ? {zoom: scale} : undefined}>
                        <WindowToolsContext.Provider value={context}>{children}</WindowToolsContext.Provider>
                    </div>
                </div>
            </Frame>
            <div className="preview-window__bar" onPointerDown={startMove} onDoubleClick={toggleMaximized}/>
            <Div className="preview-window__buttons">
                {onDownload ? (
                    <Button variant="secondary" title="Download (d)" onClicked={onDownload}><Download width={13} height={13}/></Button>
                ) : null}
                <Button variant="secondary" title={maximized ? 'Restore (m)' : 'Maximize (m)'} onClicked={toggleMaximized}>
                    {maximized ? <Collapse width={13} height={13}/> : <Expand width={13} height={13}/>}
                </Button>
                <Button variant="secondary" title="Close (esc)" onClicked={onClose}><Close width={13} height={13}/></Button>
            </Div>
            {maximized ? null : EDGES.map((edge) => (
                <div key={edge} className={`preview-window__edge preview-window__edge--${edge}`} onPointerDown={startResize(edge)}/>
            ))}
        </div>
    )
};
