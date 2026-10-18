import {useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type FocusEvent, type PointerEvent as ReactPointerEvent, type ReactNode} from 'react'
import '../../styles/tui-canvas.scss'
import {Div} from '../../components/primitives'
import {useKeyLayer} from '../../hooks/use-key-layer.ts'
import {readColors} from '../core/color.ts'
import {watchPixelRatio} from '../core/pixel-ratio.ts'
import {TuiScene} from '../core/scene.ts'
import {TuiRenderMode, type TuiPointerEvent} from '../core/types.ts'
import {TuiWidget} from '../core/widget.ts'
import {TuiSceneContext} from './use-tui-scene.ts'

/** Frames per second of a continuous canvas, when it isn't told otherwise */
const DEFAULT_FPS = 60
/** How bright the interface draws, where a background is drawn at 1: a background is tuned to stay faint behind a
 * page, a widget is what the user reads */
const INTERFACE_STRENGTH = 2
/** The longest stretch one frame counts for, so a canvas that was paused carries on instead of jumping ahead */
const MAX_FRAME_SECONDS = 0.25

export interface TuiCanvasProps {
    className?: string
    style?: CSSProperties
    /** What the canvas is, for assistive technology, which can't read what is drawn on it */
    label?: string
    /** Frames per second while `renderMode` is continuous */
    fps?: number
    /** On demand unless told otherwise: a canvas that draws only when something changed costs nothing while idle, and a
     * page may hold dozens of them. Continuous is for what moves on its own. */
    renderMode?: TuiRenderMode
    /** The faint grid of dots behind everything, at `dotAlpha` opacity */
    baseDots?: boolean
    dotAlpha?: number
    /** The brightness: 1 is how faint a background is drawn */
    strength?: number
    /** A tree built outside React, to draw instead of the one the components make */
    rootWidget?: TuiWidget
    /** Once, when the scene is made. A new function each render is fine: only the latest one is ever used. */
    onInit?: (scene: TuiScene, root: TuiWidget) => void
    /** After the widgets are drawn in every frame, for drawing that is not a widget */
    onTick?: (scene: TuiScene, dt: number, time: number) => void
    children?: ReactNode
}

/**
 * A canvas that draws a tree of widgets (`TuiBox`, `TuiText`, `TuiButton`, `TuiCustom`) on the character grid, in the
 * page's theme. It follows its container's size, lays the tree out before every frame, and hands the pointer and the
 * keyboard to the widgets: Tab walks the focusable ones, Enter or Space presses one, Escape gives the keyboard back to
 * the page. The canvas takes the keyboard only while it has the focus, so the page's own keys work around it.
 */
export const TuiCanvas = ({className, style, label, fps = DEFAULT_FPS, renderMode = TuiRenderMode.OnDemand, baseDots = false, dotAlpha, strength = INTERFACE_STRENGTH, rootWidget, onInit, onTick, children}: TuiCanvasProps) => {
    const containerRef = useRef<HTMLDivElement>(null)
    const canvasRef = useRef<HTMLCanvasElement>(null)
    const [ownRoot] = useState(() => new TuiWidget())
    const root = rootWidget ?? ownRoot
    const [scene, setScene] = useState<TuiScene | null>(null)
    const [size, setSize] = useState({cols: 0, rows: 0, width: 0, height: 0})
    const [hasKeyboard, setHasKeyboard] = useState(false)

    // The frame loop and the handlers live as long as the canvas and read what changes (the callbacks, the render mode)
    // from here, so a new function on every render can't restart them
    const latest = useRef({fps, renderMode, baseDots, dotAlpha, strength, onInit, onTick, root})
    useEffect(() => {
        latest.current = {fps, renderMode, baseDots, dotAlpha, strength, onInit, onTick, root}
    })

    const dirty = useRef(true)
    const frame = useRef(0)
    const loop = useRef<((now: number) => void) | null>(null)
    const hovered = useRef<TuiWidget | null>(null)
    const pressed = useRef<TuiWidget | null>(null)
    const focused = useRef<TuiWidget | null>(null)

    const requestRedraw = useCallback(() => {
        dirty.current = true
        if (!frame.current && loop.current) frame.current = requestAnimationFrame(loop.current)
    }, [])

    useEffect(requestRedraw, [requestRedraw, root, renderMode, baseDots, dotAlpha, strength])

    useEffect(() => {
        const canvas = canvasRef.current
        const container = containerRef.current
        const ctx = canvas?.getContext('2d')
        if (!canvas || !container || !ctx) return

        const s = new TuiScene(ctx)
        const readTheme = () => {
            s.setColors(readColors(ctx, getComputedStyle(container)))
            requestRedraw()
        }
        readTheme()
        // The theme's colors change with `data-theme` on <html>
        const themeWatch = new MutationObserver(readTheme)
        themeWatch.observe(document.documentElement, {attributes: true, attributeFilter: ['data-theme']})

        let last = performance.now()
        let acc = 0
        const tick = (now: number) => {
            frame.current = 0
            const {fps, renderMode, baseDots, dotAlpha, strength, onTick, root} = latest.current
            const continuous = renderMode === TuiRenderMode.Continuous
            let dt = Math.min(MAX_FRAME_SECONDS, (now - last) / 1000)
            last = now

            if (continuous) {
                acc += dt
                if (acc < 1 / Math.max(1, fps)) {
                    frame.current = requestAnimationFrame(tick)
                    return
                }
                dt = acc
                acc = 0
            } else if (!dirty.current) {
                return
            }
            // Nothing to draw on before the first size arrives; the size's arrival asks for a frame
            if (!s.cols) {
                if (continuous) frame.current = requestAnimationFrame(tick)
                return
            }
            dirty.current = false

            s.strength = strength
            s.begin(baseDots ? dotAlpha : null)
            root.w = s.fitCols
            root.h = s.fitRows
            root.layoutChildren({cw: s.cw, ch: s.ch})
            root.render(s, now / 1000)
            onTick?.(s, dt, now / 1000)
            s.flush()
            if (continuous) frame.current = requestAnimationFrame(tick)
        }
        loop.current = tick
        frame.current = requestAnimationFrame(tick)

        const sizeWatch = new ResizeObserver(([entry]) => {
            const {width, height} = entry.contentRect
            if (width <= 0 || height <= 0) return
            s.resize(width, height, window.devicePixelRatio || 1)
            const next = {cols: s.fitCols, rows: s.fitRows, width, height}
            setSize((cur) => cur.cols === next.cols && cur.rows === next.rows && cur.width === width && cur.height === height ? cur : next)
            requestRedraw()
        })
        sizeWatch.observe(container)
        const ratioWatch = watchPixelRatio(() => {
            if (!s.cols) return
            s.resize(s.W, s.H, window.devicePixelRatio || 1)
            requestRedraw()
        })

        setScene(s)
        latest.current.onInit?.(s, latest.current.root)

        return () => {
            cancelAnimationFrame(frame.current)
            frame.current = 0
            loop.current = null
            themeWatch.disconnect()
            sizeWatch.disconnect()
            ratioWatch()
        }
    }, [requestRedraw])

    // ---- the pointer ----

    const cellAt = (e: ReactPointerEvent<HTMLCanvasElement>) => {
        const rect = e.currentTarget.getBoundingClientRect()
        return {col: Math.floor((e.clientX - rect.left) / (scene?.cw ?? 1)), row: Math.floor((e.clientY - rect.top) / (scene?.ch ?? 1))}
    }

    /** Hands an event to `from` and then up its parents, until one stops it */
    const bubble = (from: TuiWidget | null, col: number, row: number, original: PointerEvent, call: (w: TuiWidget, e: TuiPointerEvent) => void) => {
        let stopped = false
        for (let w = from; w && !stopped; w = w.parent) {
            const pos = w.getGlobalPos()
            call(w, {col, row, localCol: col - pos.c, localRow: row - pos.r, originalEvent: original, stopPropagation: () => { stopped = true }})
        }
    }

    const setHover = (target: TuiWidget | null) => {
        if (hovered.current === target) return
        const prev = hovered.current
        hovered.current = target
        if (prev) {
            prev.isHovered = false
            prev.onHoverChange(false)
        }
        if (target) {
            target.isHovered = true
            target.onHoverChange(true)
        }
        if (canvasRef.current) canvasRef.current.style.cursor = target ? 'pointer' : ''
        requestRedraw()
    }

    const setFocus = (target: TuiWidget | null) => {
        if (focused.current === target) return
        if (focused.current) focused.current.isFocused = false
        focused.current = target
        if (target) target.isFocused = true
        requestRedraw()
    }

    const onPointerDown = (e: ReactPointerEvent<HTMLCanvasElement>) => {
        // The widget that was pressed gets the release wherever the pointer ends up
        e.currentTarget.setPointerCapture(e.pointerId)
        const {col, row} = cellAt(e)
        const hit = root.hitTest(col, row)
        pressed.current = hit
        setFocus(hit?.nearestFocusable() ?? null)
        bubble(hit, col, row, e.nativeEvent, (w, ev) => w.onPointerDown(ev))
        requestRedraw()
    }

    const onPointerMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
        const {col, row} = cellAt(e)
        const hit = root.hitTest(col, row)
        setHover(hit?.nearestClickable() ?? null)
        bubble(hit, col, row, e.nativeEvent, (w, ev) => w.onPointerMove(ev))
    }

    const onPointerUp = (e: ReactPointerEvent<HTMLCanvasElement>) => {
        if (e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId)
        const {col, row} = cellAt(e)
        const down = pressed.current
        pressed.current = null
        bubble(down, col, row, e.nativeEvent, (w, ev) => w.onPointerUp(ev))
        // A click is a press and a release on the same clickable widget (or on two parts of one)
        const target = down?.nearestClickable()
        if (target && target === root.hitTest(col, row)?.nearestClickable()) target.onClick()
        requestRedraw()
    }

    /** The browser took the pointer for something else (a scroll on a phone): the press ends, with no click */
    const onPointerCancel = (e: ReactPointerEvent<HTMLCanvasElement>) => {
        const down = pressed.current
        pressed.current = null
        bubble(down, 0, 0, e.nativeEvent, (w, ev) => w.onPointerUp(ev))
        setHover(null)
        requestRedraw()
    }

    const onPointerLeave = () => {
        if (!pressed.current) setHover(null)
    }

    // ---- the keyboard ----

    useKeyLayer(hasKeyboard, (e) => {
        const current = focused.current
        if (e.key === 'Tab') {
            const stops = latest.current.root.focusables()
            const next = (current ? stops.indexOf(current) : -1) + (e.shiftKey ? -1 : 1)
            if (next < 0 || next >= stops.length) {
                // Past either end the browser carries the focus out of the canvas, which hands the keyboard back
                setFocus(null)
                return false
            }
            setFocus(stops[next])
            return true
        }
        if (e.key === 'Escape') {
            setFocus(null)
            canvasRef.current?.blur()
            return true
        }
        return current?.onKey(e) ?? false
    })

    const onFocus = (e: FocusEvent<HTMLCanvasElement>) => {
        setHasKeyboard(true)
        // Arriving by Tab lands on the first stop; arriving by a click leaves the choice to the click
        if (!focused.current && e.currentTarget.matches(':focus-visible')) setFocus(root.focusables()[0] ?? null)
    }

    const onBlur = () => {
        setHasKeyboard(false)
        setFocus(null)
    }

    const context = useMemo(
        () => ({scene, rootWidget: root, ...size, requestRedraw}),
        [scene, root, size, requestRedraw],
    )

    return (
        <TuiSceneContext.Provider value={context}>
            <Div ref={containerRef} className={className ? `tui-canvas ${className}` : 'tui-canvas'} style={style}>
                <canvas
                    ref={canvasRef}
                    tabIndex={0}
                    aria-label={label}
                    onPointerDown={onPointerDown}
                    onPointerMove={onPointerMove}
                    onPointerUp={onPointerUp}
                    onPointerCancel={onPointerCancel}
                    onPointerLeave={onPointerLeave}
                    onFocus={onFocus}
                    onBlur={onBlur}
                />
                {children}
            </Div>
        </TuiSceneContext.Provider>
    )
};
