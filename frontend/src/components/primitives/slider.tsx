import type {CSSProperties} from 'react'
import type {SliderProps, ThemedProps} from './types'

export const Slider = ({
    style,
    className,
    variant = 'secondary',
    value,
    onChanged,
    min,
    max,
    step = 1,
    disabled,
}: ThemedProps<SliderProps>) => (
    <input
        type="range"
        // How far the value is along the track, for the filled part (only Firefox can style it on its own)
        style={{...style, '--fill': `${((value - min) / (max - min)) * 100}%`} as CSSProperties}
        className={className}
        data-variant={variant}
        value={value}
        min={min}
        max={max}
        step={step}
        disabled={disabled}
        onChange={(e) => onChanged(Number(e.target.value))}
    />
);
