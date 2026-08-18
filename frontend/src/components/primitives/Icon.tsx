import type { IconProps, ThemedProps } from './types'

export function Icon({ style, className, variant = 'secondary', src }: ThemedProps<IconProps>) {
  return <img src={src} style={style} className={className} data-variant={variant} alt="" />
}
