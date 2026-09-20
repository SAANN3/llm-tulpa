import type {CheckboxProps, ThemedProps} from './types'

export const Checkbox = ({
     style,
     className,
     variant = 'secondary',
     toggled,
     onToggled,
     name,
 }: ThemedProps<CheckboxProps>) => (
    <input
        type="checkbox"
        style={style}
        className={className}
        data-variant={variant}
        name={name}
        checked={toggled}
        onChange={(e) => onToggled(e.target.checked)}
    />
);
