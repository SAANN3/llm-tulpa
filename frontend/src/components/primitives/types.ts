import type { CSSProperties, DragEvent, KeyboardEvent, MouseEvent, ReactNode, UIEvent } from 'react'

export interface OverrideThemeParams {
  style?: CSSProperties
  className?: string
  variant?: 'primary' | 'secondary' | 'tertiary'
}

export type ThemedProps<T> = T & OverrideThemeParams

export interface DivProps {
  children?: ReactNode
  onClick?: (e: MouseEvent<HTMLDivElement>) => void
  onContextMenu?: (e: MouseEvent<HTMLDivElement>) => void
  onMouseDown?: (e: MouseEvent<HTMLDivElement>) => void
  onHover?: (hovering: boolean) => void
  onScroll?: (e: UIEvent<HTMLDivElement>) => void
  onDragOver?: (e: DragEvent<HTMLDivElement>) => void
  onDragEnter?: (e: DragEvent<HTMLDivElement>) => void
  onDragLeave?: (e: DragEvent<HTMLDivElement>) => void
  onDrop?: (e: DragEvent<HTMLDivElement>) => void
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
