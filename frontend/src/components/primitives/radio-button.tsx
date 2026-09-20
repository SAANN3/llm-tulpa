import type {RadioButtonProps, ThemedProps} from './types'

export const RadioButton = ({
    style,
    className,
    variant = 'secondary',
    name,
    value,
    checked,
    onChanged,
}: ThemedProps<RadioButtonProps>) => (
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
);
