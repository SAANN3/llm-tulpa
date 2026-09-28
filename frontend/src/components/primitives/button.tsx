import type {ButtonProps, ThemedProps} from './types'

export const Button = ({
    style,
    className,
    variant = "primary",
    text,
    title,
    onClicked,
    children,
    disabled
}: ThemedProps<ButtonProps>) => (
    <button
        type="button"
        style={style}
        className={className}
        data-variant={variant}
        title={title}
        onClick={onClicked}
        disabled={disabled}
    >
        {text}
        {children}
    </button>
);
