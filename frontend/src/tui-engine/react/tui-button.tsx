import {useTuiWidget, type TuiComponentProps} from './use-tui-widget.ts'
import type {Level} from '../core/types.ts'
import {TuiButtonWidget} from '../primitives/button.ts'

export interface TuiButtonProps extends TuiComponentProps {
    label: string
    /** The level while the pointer is over it, it is pressed or it has the focus */
    activeLevel?: Level
}

export const TuiButton = ({label, level = 3, activeLevel = 5, ...rest}: TuiButtonProps) => {
    const props = {...rest, level}
    useTuiWidget(
        () => new TuiButtonWidget({...props, label, activeLevel}),
        props,
        (w) => {
            w.label = label
            w.activeLevel = activeLevel
        },
    )
    return null
};
