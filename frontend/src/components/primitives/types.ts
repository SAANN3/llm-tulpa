import type { CSSProperties, KeyboardEvent, MouseEvent, ReactNode, UIEvent } from 'react'

/**
 * Style/class overrides every themed component accepts on top of its own props.
 * `variant` is forwarded as a `data-variant` DOM attribute, not resolved to a color here —
 * each theme's own CSS decides what (if anything) a variant looks like. See
 * `../THEMING.md` for the rules on where variant colors are (and aren't) allowed to be
 * defined — getting this wrong causes silent collisions between themes.
 */
export interface OverrideThemeParams {
  style?: CSSProperties
  className?: string
  variant?: 'primary' | 'secondary' | 'tertiary'
}

/** A component's own props, flattened together with the override params every themed component accepts. */
export type ThemedProps<T> = T & OverrideThemeParams

export interface DivProps {
  children?: ReactNode
  onClick?: (e: MouseEvent<HTMLDivElement>) => void
  onContextMenu?: (e: MouseEvent<HTMLDivElement>) => void
  onMouseDown?: (e: MouseEvent<HTMLDivElement>) => void
  onHover?: (hovering: boolean) => void
  onScroll?: (e: UIEvent<HTMLDivElement>) => void
}

export interface LabelProps {
  text: string
}

export interface ButtonProps {
  text?: string
  onClicked: () => void
  children?: ReactNode
  disabled?: boolean
}

export interface InputProps {
  text: string
  onChanged: (text: string) => void
  placeholder?: string
  onHovered?: (hovering: boolean) => void
  onKeyDown?: (e: KeyboardEvent<HTMLInputElement>) => void
}

export interface TextFieldProps {
  text: string
  onChanged: (text: string) => void
  placeholder?: string
  disabled?: boolean
  onHovered?: (hovering: boolean) => void
  onKeyDown?: (e: KeyboardEvent<HTMLTextAreaElement>) => void
}

export interface SelectProps {
  readonly values: string[]
  selected?: string
  onChosen: (value: string) => void
}

export interface RadioButtonProps {
  name?: string
  value: string
  checked: boolean
  onChanged: (value: string) => void
}

export interface CheckboxProps {
  toggled: boolean
  onToggled: (toggled: boolean) => void
  name?: string
}

export interface ToggleSwitchProps {
  toggled: boolean
  onToggled: (toggled: boolean) => void
  disabled?: boolean
}

export interface IconProps {
  src: string
}
