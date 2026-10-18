import {ParentWidgetContext} from './use-tui-scene.ts'
import {useTuiWidget, type TuiComponentProps} from './use-tui-widget.ts'
import type {BorderStyle} from '../core/types.ts'
import {TuiBoxWidget} from '../primitives/box.ts'

export interface TuiBoxProps extends TuiComponentProps {
    title?: string
    borderStyle?: BorderStyle
    fill?: boolean
}

/** A frame; what is inside it is placed inside the frame */
export const TuiBox = ({children, title = '', borderStyle = 'double', fill = true, level = 4, ...rest}: TuiBoxProps) => {
    const props = {...rest, level}
    const widget = useTuiWidget(
        () => new TuiBoxWidget({...props, title, borderStyle, fill}),
        props,
        (w) => {
            w.title = title
            w.borderStyle = borderStyle
            w.fill = fill
        },
    )
    return <ParentWidgetContext.Provider value={widget}>{children}</ParentWidgetContext.Provider>
};
