import type { ThemedProps, ToggleSwitchProps } from './types'

export function ToggleSwitch({
  style,
  className,
  variant = 'secondary',
  toggled,
  onToggled,
}: ThemedProps<ToggleSwitchProps>) {
  return (
    <input
      type="checkbox"
      role="switch"
      style={style}
      className={className}
      data-variant={variant}
      checked={toggled}
      onChange={(e) => onToggled(e.target.checked)}
    />
  )
}
