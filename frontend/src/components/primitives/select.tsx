import {useEffect, useRef, useState} from 'react'
import type {SelectProps, ThemedProps} from './types'

const PANEL_MAX_HEIGHT = 240
const PANEL_MARGIN = 4

export const Select = ({
    style,
    className,
    variant = 'secondary',
    values,
    selected,
    onChosen
}: ThemedProps<SelectProps>) => {
    const [open, setOpen] = useState(false)
    const [direction, setDirection] = useState<'up' | 'down'>('down')
    const rootRef = useRef<HTMLDivElement>(null)

    useEffect(() => {
        if (!open) return

        function onPointerDown(e: PointerEvent) {
            if (!rootRef.current?.contains(e.target as Node)) setOpen(false)
        }

        function onKeyDown(e: KeyboardEvent) {
            if (e.key === 'Escape') setOpen(false)
        }

        document.addEventListener('pointerdown', onPointerDown)
        document.addEventListener('keydown', onKeyDown)
        return () => {
            document.removeEventListener('pointerdown', onPointerDown)
            document.removeEventListener('keydown', onKeyDown)
        }
    }, [open])

    const toggleOpen = () => {
        if (open) {
            setOpen(false)
            return
        }

        const rect = rootRef.current?.getBoundingClientRect()
        if (rect) {
            const spaceBelow = window.innerHeight - rect.bottom
            const spaceAbove = rect.top
            const fitsBelow = spaceBelow >= PANEL_MAX_HEIGHT + PANEL_MARGIN
            setDirection(!fitsBelow && spaceAbove > spaceBelow ? 'up' : 'down')
        }
        setOpen(true)
    }

    return (
        <div ref={rootRef} style={style} className={className} data-variant={variant} data-select>
            <button
                type="button"
                data-select-trigger
                aria-haspopup="listbox"
                aria-expanded={open}
                onClick={toggleOpen}
            >
                {selected ?? ''}
            </button>
            {open && (
                <ul data-select-panel data-direction={direction} role="listbox">
                    {values.map((value) => (
                        <li
                            key={value}
                            role="option"
                            aria-selected={value === selected}
                            data-select-option
                            onClick={() => {
                                onChosen(value)
                                setOpen(false)
                            }}
                        >
                            {value}
                        </li>
                    ))}
                </ul>
            )}
        </div>
    )
};
