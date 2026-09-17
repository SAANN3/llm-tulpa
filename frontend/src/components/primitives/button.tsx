import type {ButtonProps, ThemedProps} from './types'

export const Button = ({
    style,
    className,
    variant = "primary",
    text,
    onClicked,
    children,
    disabled
}: ThemedProps<ButtonProps>) => (
    <button
        type="button"
        style={style}
        className={className}
        data-variant={variant}
        onClick={onClicked}
        disabled={disabled}
    >
        {text}
        {children}
    </button>
);
