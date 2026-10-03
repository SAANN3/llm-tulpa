import {useEffect, useRef} from 'react'
import type {ReactNode} from 'react'
import {createPortal} from 'react-dom'
import '../../../styles/popup.scss'
import {useEscapeToClose} from '../../../hooks/use-escape-to-close.ts'
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
    position?: undefined
}

/** A menu: unframed and anchored at a point, such as a right-click or a "more" button */
interface MenuProps extends PopupBaseProps {
    position: { x: number; y: number }
    title?: undefined
    width?: undefined
}

export type PopupProps = DialogProps | MenuProps

/** The one overlay every popup is built from. It closes itself on an outside click or Escape.
 * A dialog (no `position`) gets the shared look: a titled frame with a minimum size, and a
 * body that spaces whatever it holds evenly, so a popup's content only has to bring its rows. */
export const Popup = (props: PopupProps) => {
    const {open, onClose, children} = props
    const ref = useRef<HTMLDivElement>(null)

    useEscapeToClose(open, onClose)

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
                 style={{left: props.position.x, top: props.position.y}}>
                {children}
            </Div>,
            document.body
        )
    }

    return createPortal(
        <Div ref={ref} variant="secondary" className="vbox popup popup--dialog">
            <Div className="dos-frame popup__frame" style={{width: props.width}}>
                <span className="dos-frame__title">{props.title}</span>
                <Div className="dos-frame__body popup__body">{children}</Div>
            </Div>
        </Div>,
        document.body
    )
};
