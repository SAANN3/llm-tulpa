import type { ButtonProps, ThemedProps } from './types'

export function Button({
  style,
  className,
  variant = "primary",
  text,
  onClicked,
  children,
  disabled,
}: ThemedProps<ButtonProps>) {
  return (
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
  )
}
