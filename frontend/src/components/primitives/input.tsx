import type {InputProps, ThemedProps} from './types'

export const Input = ({
    style,
    className,
    variant = 'secondary',
    text,
    onChanged,
    placeholder,
    type = 'text',
    autoFocus,
    onHovered,
    onKeyDown,
    onBlur,
    inputRef,
}: ThemedProps<InputProps>) => (
    <input
        ref={inputRef}
        type={type}
        style={style}
        className={className}
        data-variant={variant}
        value={text}
        placeholder={placeholder}
        autoFocus={autoFocus}
        onChange={(e) => onChanged(e.target.value)}
        onMouseEnter={() => onHovered?.(true)}
        onMouseLeave={() => onHovered?.(false)}
        onKeyDown={onKeyDown}
        onBlur={onBlur}
    />
);
