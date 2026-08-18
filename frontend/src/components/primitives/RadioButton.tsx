import type { RadioButtonProps, ThemedProps } from './types'

export function RadioButton({
  style,
  className,
  variant = 'secondary',
  name,
  value,
  checked,
  onChanged,
}: ThemedProps<RadioButtonProps>) {
  return (
    <input
      type="radio"
      style={style}
      className={className}
      data-variant={variant}
      name={name}
      value={value}
      checked={checked}
      onChange={() => onChanged(value)}
    />
  )
}
