import type {TuiScene} from '../core/scene.ts'
import type {Level, TuiWidgetOpts} from '../core/types.ts'
import {TuiWidget} from '../core/widget.ts'

/** What an area graph is set with; the widget and its React component take the same options */
export interface AreaGraphOptions {
    /** One height per dot column, oldest first; the newest is at the right edge and a graph with fewer than it has room for starts there. A cell holds two dot columns. */
    values?: number[]
    /** The value that reaches the top of the graph; the largest of `values` (at least 1) when not given */
    max?: number
}

export interface TuiAreaGraphOpts extends TuiWidgetOpts, AreaGraphOptions {}

/** Braille is drawn with eight dots to a cell, in two columns of four: the bit of each, from the bottom dot up */
const LEFT_DOTS = [0x40, 0x04, 0x02, 0x01]
const RIGHT_DOTS = [0x80, 0x20, 0x10, 0x08]
const BRAILLE_BASE = 0x2800
const DOTS_PER_ROW = 4
const DOT_COLUMNS_PER_CELL = 2
/** The area under the line, and the line itself */
const AREA_LEVEL: Level = 2
const LINE_LEVEL: Level = 5

/**
 * A graph of values over time as an area under a brighter line, drawn with braille dots: two columns of them to a
 * cell and four rows, so it is finer than a bar graph of characters. The font has no braille; the scene draws it
 * from another font, each symbol fitted to its cell.
 */
export class TuiAreaGraphWidget extends TuiWidget {
    values: number[] = []
    max?: number

    constructor(opts: TuiAreaGraphOpts = {}) {
        super(opts)
        this.configure(opts)
    }

    /** Takes the graph's own options, each falling back to its default when missing */
    configure(opts: AreaGraphOptions): void {
        this.values = opts.values ?? []
        this.max = opts.max
    }

    override onRenderSelf(scene: TuiScene): void {
        if (this.w <= 0 || this.h <= 0) return
        const dotColumns = this.w * DOT_COLUMNS_PER_CELL
        const shown = this.values.slice(-dotColumns)
        const first = dotColumns - shown.length
        const top = this.max ?? Math.max(1, ...shown)
        const height = (dotColumn: number) => (dotColumn < first ? null : Math.min(1, shown[dotColumn - first] / top))
        this.drawLayer(scene, height, true, AREA_LEVEL)
        this.drawLayer(scene, height, false, LINE_LEVEL)
    }

    /** One layer of dots: every dot up to the height of its column (`fill`), or only the one at the height */
    private drawLayer(scene: TuiScene, height: (dotColumn: number) => number | null, fill: boolean, level: Level): void {
        const {r, c} = this.getGlobalPos()
        const dotRows = this.h * DOTS_PER_ROW
        for (let col = 0; col < this.w; col++) {
            for (let row = 0; row < this.h; row++) {
                let bits = 0
                for (let side = 0; side < DOT_COLUMNS_PER_CELL; side++) {
                    const v = height(col * DOT_COLUMNS_PER_CELL + side)
                    if (v == null) continue
                    const top = Math.max(1, Math.round(v * dotRows))
                    for (let dot = 0; dot < DOTS_PER_ROW; dot++) {
                        const dotRow = (this.h - 1 - row) * DOTS_PER_ROW + dot + 1
                        if (fill ? dotRow <= top : dotRow === top) bits |= (side === 0 ? LEFT_DOTS : RIGHT_DOTS)[dot]
                    }
                }
                if (bits) scene.put(r + row, c + col, String.fromCharCode(BRAILLE_BASE + bits), level)
            }
        }
    }
}
