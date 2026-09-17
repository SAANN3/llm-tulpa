import {useEffect, useRef} from 'react'
import type {ReactNode} from 'react'
import '../styles/popup.scss'
import {Div} from './primitives'

export interface PopupProps {
    open: boolean
    onClose: () => void
    position?: { x: number; y: number }
    centered?: boolean
    children: ReactNode
}

/** A themed, positioned overlay that closes itself on an outside click */
export const Popup = ({open, onClose, position, centered, children}: PopupProps) => {
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
        ? {left: '50%', top: '50%', transform: 'translate(-50%, -50%)'}
        : {left: position?.x ?? 0, top: position?.y ?? 0}

    return (
        <Div ref={ref} variant="secondary" className="vbox popup" style={placement}>
            {children}
        </Div>
    )
};
