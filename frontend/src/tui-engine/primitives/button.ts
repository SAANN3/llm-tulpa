import type {TuiScene} from '../core/scene.ts'
import {textWidth} from '../core/text-cells.ts'
import type {Level, TuiPointerEvent, TuiWidgetOpts} from '../core/types.ts'
import {TuiWidget} from '../core/widget.ts'

export interface TuiButtonWidgetOpts extends TuiWidgetOpts {
    label: string
    activeLevel?: Level
}

/** A one-row label that lights up while the pointer is over it, while it is pressed and while it has the focus; the
 * click itself is the canvas's (`onClick`) */
export class TuiButtonWidget extends TuiWidget {
    label: string
    activeLevel: Level
    private pressed = false

    constructor(opts: TuiButtonWidgetOpts) {
        super(opts)
        this.label = opts.label
        this.level = opts.level ?? 3
        this.activeLevel = opts.activeLevel ?? 5
    }

    override measure(): { w: number; h: number } {
        return {w: textWidth(this.label), h: 1}
    }

    override onPointerDown(_e: TuiPointerEvent): void {
        this.pressed = true
    }

    override onPointerUp(_e: TuiPointerEvent): void {
        this.pressed = false
    }

    override onRenderSelf(scene: TuiScene): void {
        const {r, c} = this.getGlobalPos()
        const lit = this.pressed || this.isHovered || this.isFocused
        scene.write(r, c, this.label, lit ? this.activeLevel : this.level)
    }
}
