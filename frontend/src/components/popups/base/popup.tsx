import {useEffect, useLayoutEffect, useRef, useState} from 'react'
import type {ReactNode} from 'react'
import {createPortal} from 'react-dom'
import '../../../styles/popup.scss'
import {useEscapeToClose} from '../../../hooks/use-escape-to-close.ts'
import {Frame, type FrameAction, type FrameTab} from '../../frame.tsx'
import {Div} from '../../primitives'

interface PopupBaseProps {
    open: boolean
    onClose: () => void
    children: ReactNode
}

/** A dialog: centered, in a titled frame */
interface DialogProps extends PopupBaseProps {
    /** The label on the frame's top edge — every dialog has one */
    title: string
    /** Overrides the default width; a dialog is never narrower than its minimum either way */
    width?: number
    /** What the dialog's keys do besides Escape (which closes it) and its tab keys: the line along its bottom is built
     * from these. Required, so every dialog says what it offers; an empty list says there is nothing more. */
    actions: FrameAction[]
    /** Tabs on the frame's top edge in place of the title, numbered and switched with keys 1 to 9 */
    tabs?: FrameTab[]
    activeTab?: string
    onTab?: (id: string) => void
    /** Explicitly show a close button [×] on the frame border (useful for popups that lack cancel/close buttons) */
    showCloseButton?: boolean
    position?: undefined
    corner?: undefined
    minWidth?: undefined
}

/** A menu: unframed and anchored at a point, such as a right-click or a "more" button */
interface MenuProps extends PopupBaseProps {
    position: { x: number; y: number }
    /** Which corner of the menu sits at `position`: the top left (the default) opens it below and to the right,
     * the bottom right above and to the left, the bottom left above and to the right (for a control at the bottom of
     * the page: at its right edge, or its left) */
    corner?: 'top-left' | 'bottom-right' | 'bottom-left'
    /** Never narrower than this many pixels (the control it opens from, so it reads as that control's list); it is
     * as wide as its items otherwise */
    minWidth?: number
    className?: string
    title?: undefined
    width?: undefined
    actions?: undefined
    tabs?: undefined
    showCloseButton?: undefined
}

export type PopupProps = DialogProps | MenuProps

/** A menu is kept this far from every edge of the window */
export const EDGE_GAP = 8
/** Room a menu anchored to a corner needs on the side it grows toward; with less, it opens the other way */
const CORNER_ROOM = 260
/** Room a menu opened at a point needs to its right; with less, it grows leftward from the point */
const SIDE_ROOM = 200
/** Room a menu opened at a point needs below it; with less, it grows upward */
const BELOW_ROOM = 170
/** On a window this narrow or narrower a menu that doesn't fit is centered rather than pushed to one side */
const CENTER_AT_WIDTH = 600

/** The one overlay every popup is built from. It closes itself on an outside click or Escape.
 * A dialog (no `position`) gets the shared look: the app's `Frame`, titled or with tabs, with a minimum size, a body
 * that spaces whatever it holds evenly (so a popup's content only has to bring its rows) and the line of its keys. */
export const Popup = (props: PopupProps) => {
    const {open, onClose, children} = props
    const ref = useRef<HTMLDivElement>(null)
    const [adjustedLeft, setAdjustedLeft] = useState<number | null>(null)

    // A dialog's Frame handles Escape along with its other keys; a menu has no frame
    useEscapeToClose(open && props.position != null, onClose)

    // Only one menu is open at a time: on a touch screen a second one opens while the first is still up. The latest
    // `onClose` is read through a ref, because callers pass a new function on every render and an effect that depended
    // on it would announce "close the others" again each time its parent redrew.
    const onCloseRef = useRef(onClose)
    useEffect(() => {
        onCloseRef.current = onClose
    })
    const isMenu = props.position != null
    useEffect(() => {
        if (!open || !isMenu) return

        window.dispatchEvent(new CustomEvent('tulpa:close-menu'))

        const handleCloseOther = () => onCloseRef.current()

        window.addEventListener('tulpa:close-menu', handleCloseOther)
        return () => window.removeEventListener('tulpa:close-menu', handleCloseOther)
    }, [open, isMenu])

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

    useEffect(() => {
        if (!open) setAdjustedLeft(null)
    }, [open])

    useLayoutEffect(() => {
        if (!open || !props.position || !ref.current) return
        const el = ref.current
        const rect = el.getBoundingClientRect()
        const winW = window.innerWidth
        const pad = EDGE_GAP

        const overflowsRight = rect.right > winW - pad
        const overflowsLeft = rect.left < pad

        if (overflowsRight || overflowsLeft) {
            const width = rect.width
            // If the menu is wider than the viewport minus margins (or nearly so),
            // or on a window no wider than CENTER_AT_WIDTH, center it horizontally across the screen
            if (width >= winW - pad * 2 - 24 || winW <= CENTER_AT_WIDTH) {
                setAdjustedLeft(Math.max(pad, Math.round((winW - width) / 2)))
            } else if (overflowsRight) {
                // Shift left so it stays entirely on screen
                setAdjustedLeft(Math.max(pad, Math.round(winW - pad - width)))
            } else if (overflowsLeft) {
                // Shift right so it stays entirely on screen
                setAdjustedLeft(pad)
            }
        }
    }, [open, props.position, props.corner])

    if (!open) return null

    // Rendered into <body>: `position: fixed` is relative to the nearest transformed ancestor, so
    // inside the setup wizard's sliding track (a transform, clipped) a popup opened off-screen
    if (props.position) {
        const pad = EDGE_GAP
        const x = Math.max(pad, Math.min(props.position.x, window.innerWidth - pad))
        const y = Math.max(pad, Math.min(props.position.y, window.innerHeight - pad))

        // On mobile or narrow viewports, avoid anchoring bottom-right if x is too small (x < CORNER_ROOM),
        // because it extends leftwards and clips off the left screen edge.
        // Similarly, if bottom-left is requested but x is too close to the right edge, flip it.
        let corner = props.corner
        if (corner === 'bottom-right' && x < CORNER_ROOM) {
            corner = 'bottom-left'
        } else if (corner === 'bottom-left' && x > window.innerWidth - CORNER_ROOM) {
            corner = 'bottom-right'
        }

        const isNearRight = !corner && x > window.innerWidth - SIDE_ROOM
        const isNearBottom = !corner && y > window.innerHeight - BELOW_ROOM

        return createPortal(
            <>
                <div
                    className="popup-menu__backdrop"
                    onClick={(e) => {
                        e.stopPropagation()
                        onClose()
                    }}
                    onPointerDown={(e) => {
                        e.stopPropagation()
                        onClose()
                    }}
                    onContextMenu={(e) => {
                        e.preventDefault()
                        onClose()
                    }}
                />
                <Div ref={ref} variant="secondary" className={`vbox popup popup--menu${props.className ? ` ${props.className}` : ''}`}
                     style={{
                         ...(corner === 'bottom-right'
                             ? {right: Math.max(pad, window.innerWidth - x), bottom: window.innerHeight - y}
                             : corner === 'bottom-left'
                                 ? {left: Math.max(pad, x), bottom: window.innerHeight - y}
                                 : {
                                     ...(isNearRight ? {right: Math.max(pad, window.innerWidth - x)} : {left: Math.max(pad, x)}),
                                     ...(isNearBottom ? {bottom: Math.max(pad, window.innerHeight - y)} : {top: Math.max(pad, y)}),
                                 }),
                         ...(adjustedLeft != null ? {left: adjustedLeft, right: 'auto'} : {}),
                         minWidth: props.minWidth,
                         maxWidth: `calc(100vw - ${pad * 2}px)`,
                     }}>
                    {children}
                </Div>
            </>,
            document.body
        )
    }

    return createPortal(
        <>
            <div className="popup__backdrop" onClick={onClose} />
            <Div ref={ref} variant="secondary" className="vbox popup popup--dialog">
                <Frame title={props.title} tabs={props.tabs} activeTab={props.activeTab} onTab={props.onTab}
                       actions={props.actions} onEscape={onClose} escapeLabel="close"
                       showCloseButton={props.showCloseButton} onClose={onClose}
                       className="popup__dialog" boxClassName="popup__frame" bodyClassName="popup__body"
                       style={{width: props.width}}>
                    {children}
                </Frame>
            </Div>
        </>,
        document.body
    )
};
