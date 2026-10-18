import type {TuiScene} from '../core/scene.ts'
import {formatTextLines, textWidth} from '../core/text-cells.ts'
import type {Level, TuiWidgetOpts} from '../core/types.ts'
import {TuiWidget} from '../core/widget.ts'

export interface DrawTextOptions {
    level?: Level
    /** The most cells a line may take; without it a line is as long as it is */
    maxCols?: number
    truncate?: boolean
    wrap?: boolean
}

/** What happens to a line longer than the room: wrapped onto more lines, cut with `…`, or cut off */
const overflowMode = (wrap?: boolean, truncate?: boolean): 'wrap' | 'truncate' | 'clip' => wrap ? 'wrap' : truncate ? 'truncate' : 'clip'

/** Draws a text, which may have line breaks, from row `r`, column `c`; returns the lines it took */
export function drawText(scene: TuiScene, r: number, c: number, text: string, options?: DrawTextOptions): number {
    const lines = formatTextLines(text, options?.maxCols, overflowMode(options?.wrap, options?.truncate))
    lines.forEach((line, i) => scene.write(r + i, c, line, options?.level ?? 5))
    return lines.length
}

export interface TuiTextWidgetOpts extends TuiWidgetOpts {
    text: string
    truncate?: boolean
    wrap?: boolean
}

/** Text of one or more lines; without a width it is as wide as its longest line, and a wrapping or truncating one
 * takes the room its parent has */
export class TuiTextWidget extends TuiWidget {
    text: string
    truncate: boolean
    wrap: boolean

    constructor(opts: TuiTextWidgetOpts) {
        super(opts)
        this.text = opts.text
        this.level = opts.level ?? 5
        this.truncate = opts.truncate ?? false
        this.wrap = opts.wrap ?? false
    }

    override measure(fixedW: number | null, availCols: number): { w: number; h: number } {
        const limit = fixedW ?? (this.wrap || this.truncate ? availCols : undefined)
        const lines = formatTextLines(this.text, limit, overflowMode(this.wrap, this.truncate))
        return {w: fixedW ?? Math.max(1, ...lines.map(textWidth)), h: Math.max(1, lines.length)}
    }

    override onRenderSelf(scene: TuiScene): void {
        const {r, c} = this.getGlobalPos()
        drawText(scene, r, c, this.text, {level: this.level, maxCols: this.w, truncate: this.truncate, wrap: this.wrap})
    }
}
