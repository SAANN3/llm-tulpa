import type { CheckboxProps, ThemedProps } from './types'

export function Checkbox({
  style,
  className,
  variant = 'secondary',
  toggled,
  onToggled,
  name,
}: ThemedProps<CheckboxProps>) {
  return (
    <input
      type="checkbox"
      style={style}
      className={className}
      data-variant={variant}
      name={name}
      checked={toggled}
      onChange={(e) => onToggled(e.target.checked)}
    />
  )
}
