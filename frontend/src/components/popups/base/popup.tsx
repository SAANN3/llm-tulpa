import {useEffect, useRef} from 'react'
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
    position?: undefined
    corner?: undefined
    minWidth?: undefined
}

/** A menu: unframed and anchored at a point, such as a right-click or a "more" button */
interface MenuProps extends PopupBaseProps {
    position: { x: number; y: number }
    /** Which corner of the menu sits at `position`: the top left (the default) opens it below and to the right,
     * the bottom right opens it above and to the left, for a control at the bottom of the page */
    corner?: 'top-left' | 'bottom-right'
    /** Never narrower than this many pixels (the control it opens from, so it reads as that control's list); it is
     * as wide as its items otherwise */
    minWidth?: number
    title?: undefined
    width?: undefined
    actions?: undefined
    tabs?: undefined
}

export type PopupProps = DialogProps | MenuProps

/** The one overlay every popup is built from. It closes itself on an outside click or Escape.
 * A dialog (no `position`) gets the shared look: the app's `Frame`, titled or with tabs, with a minimum size, a body
 * that spaces whatever it holds evenly (so a popup's content only has to bring its rows) and the line of its keys. */
export const Popup = (props: PopupProps) => {
    const {open, onClose, children} = props
    const ref = useRef<HTMLDivElement>(null)

    // A dialog's Frame handles Escape along with its other keys; a menu has no frame
    useEscapeToClose(open && props.position != null, onClose)

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

    // Rendered into <body>: `position: fixed` is relative to the nearest transformed ancestor, so
    // inside the setup wizard's sliding track (a transform, clipped) a popup opened off-screen
    if (props.position) {
        return createPortal(
            <Div ref={ref} variant="secondary" className="vbox popup popup--menu"
                 style={{
                     ...(props.corner === 'bottom-right'
                         ? {right: window.innerWidth - props.position.x, bottom: window.innerHeight - props.position.y}
                         : {left: props.position.x, top: props.position.y}),
                     minWidth: props.minWidth,
                 }}>
                {children}
            </Div>,
            document.body
        )
    }

    return createPortal(
        <Div ref={ref} variant="secondary" className="vbox popup popup--dialog">
            <Frame title={props.title} tabs={props.tabs} activeTab={props.activeTab} onTab={props.onTab}
                   actions={props.actions} onEscape={onClose} escapeLabel="close"
                   className="popup__dialog" boxClassName="popup__frame" bodyClassName="popup__body"
                   style={{width: props.width}}>
                {children}
            </Frame>
        </Div>,
        document.body
    )
};
