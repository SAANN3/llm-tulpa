import { useEffect, useRef, useState } from 'react'

import type { SelectProps, ThemedProps } from './types'

export function Select({ style, className, variant = 'secondary', values, selected, onChosen }: ThemedProps<SelectProps>) {
  const [open, setOpen] = useState(false)
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

  return (
    <div ref={rootRef} style={style} className={className} data-variant={variant} data-select>
      <button
        type="button"
        data-select-trigger
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((isOpen) => !isOpen)}
      >
        {selected ?? ''}
      </button>
      {open && (
        <ul data-select-panel role="listbox">
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
}
