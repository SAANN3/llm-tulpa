import {useTuiWidget, type TuiComponentProps} from './use-tui-widget.ts'
import {TuiTextWidget} from '../primitives/text.ts'

export interface TuiTextProps extends TuiComponentProps {
    text: string
    /** Cuts a line that is longer than the room with `…` */
    truncate?: boolean
    /** Breaks a line that is longer than the room onto the next */
    wrap?: boolean
}

export const TuiText = ({text, level = 5, truncate = false, wrap = false, ...rest}: TuiTextProps) => {
    const props = {...rest, level}
    useTuiWidget(
        () => new TuiTextWidget({...props, text, truncate, wrap}),
        props,
        (w) => {
            w.text = text
            w.truncate = truncate
            w.wrap = wrap
        },
    )
    return null
};
