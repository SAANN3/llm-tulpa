import type {LabelProps, ThemedProps} from './types'

export const Label = ({style, className, variant, text, children}: ThemedProps<LabelProps>) => (
    <span style={style} className={className} data-variant={variant} data-label>
      {text}
      {children}
    </span>
);
