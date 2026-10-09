import {useRef} from 'react'
import type {KeyboardEvent} from 'react'
import type {ChoiceGroupProps, ThemedProps} from './types'

/** One of a few options, side by side in one frame, the chosen one filled: a radio group drawn as a row of cells.
 * Arrow keys move the choice (and the focus, which only the chosen option takes, as a radio group's does). */
export const ChoiceGroup = ({
    style,
    className,
    variant = 'secondary',
    options,
    chosen,
    onChosen,
    label,
}: ThemedProps<ChoiceGroupProps>) => {
    const refs = useRef<(HTMLButtonElement | null)[]>([])
    const index = options.findIndex((o) => o.value === chosen)

    const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
        const step = e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 1 : e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -1 : 0
        if (!step) return
        e.preventDefault()
        const next = (Math.max(0, index) + step + options.length) % options.length
        onChosen(options[next].value)
        refs.current[next]?.focus()
    }

    return (
        <div role="radiogroup" aria-label={label} style={style} className={className} data-variant={variant} data-choice-group>
            {options.map((option, i) => (
                <button
                    key={option.value}
                    ref={(el) => {
                        refs.current[i] = el
                    }}
                    type="button"
                    role="radio"
                    aria-checked={option.value === chosen}
                    tabIndex={option.value === chosen || (index < 0 && i === 0) ? 0 : -1}
                    data-choice
                    onClick={() => onChosen(option.value)}
                    onKeyDown={onKeyDown}
                >
                    {option.label}
                </button>
            ))}
        </div>
    )
};
