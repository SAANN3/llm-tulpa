import type { ThemedProps, ToggleSwitchProps } from './types'

export function ToggleSwitch({
  style,
  className,
  variant = 'secondary',
  toggled,
  onToggled,
  disabled,
}: ThemedProps<ToggleSwitchProps>) {
  return (
    <input
      type="checkbox"
      role="switch"
      style={style}
      className={className}
      data-variant={variant}
      checked={toggled}
      disabled={disabled}
      onChange={(e) => onToggled(e.target.checked)}
    />
  )
}
