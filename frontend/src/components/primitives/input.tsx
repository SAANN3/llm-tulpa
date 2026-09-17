import type {InputProps, ThemedProps} from './types'

export const Input = ({
    style,
    className,
    variant = 'secondary',
    text,
    onChanged,
    placeholder,
    onHovered,
    onKeyDown
}: ThemedProps<InputProps>) => (
    <input
        type="text"
        style={style}
        className={className}
        data-variant={variant}
        value={text}
        placeholder={placeholder}
        onChange={(e) => onChanged(e.target.value)}
        onMouseEnter={() => onHovered?.(true)}
        onMouseLeave={() => onHovered?.(false)}
        onKeyDown={onKeyDown}
    />
);
