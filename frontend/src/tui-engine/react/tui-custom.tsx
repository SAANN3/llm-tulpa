import type {TuiScene} from '../core/scene.ts'
import {TuiWidget} from '../core/widget.ts'
import {useTuiWidget, type TuiComponentProps} from './use-tui-widget.ts'

export interface TuiCustomProps extends TuiComponentProps {
    /** Draws the widget into the scene; `widget` gives its place and size */
    onRender: (scene: TuiScene, time: number, widget: TuiWidget) => void
}

/** A widget that draws itself with a function, for anything the ready-made ones don't cover */
class CustomWidget extends TuiWidget {
    draw: TuiCustomProps['onRender'] = () => {}

    override onRenderSelf(scene: TuiScene, time: number): void {
        this.draw(scene, time, this)
    }
}

export const TuiCustom = ({onRender, ...rest}: TuiCustomProps) => {
    useTuiWidget(
        () => new CustomWidget(rest),
        rest,
        (w) => {
            w.draw = onRender
        },
    )
    return null
};
