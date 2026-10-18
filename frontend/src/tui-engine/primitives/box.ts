import type {TuiScene} from '../core/scene.ts'
import {truncateLine} from '../core/text-cells.ts'
import type {BorderGlyphs, BorderStyle, Level, TuiWidgetOpts} from '../core/types.ts'
import {TuiWidget} from '../core/widget.ts'

export const BORDER_SETS: Record<BorderStyle, BorderGlyphs> = {
    double: {tl: '╔', tr: '╗', bl: '╚', br: '╝', h: '═', v: '║'},
    single: {tl: '┌', tr: '┐', bl: '└', br: '┘', h: '─', v: '│'},
    rounded: {tl: '╭', tr: '╮', bl: '╰', br: '╯', h: '─', v: '│'},
    heavy: {tl: '┏', tr: '┓', bl: '┗', br: '┛', h: '━', v: '┃'},
    ascii: {tl: '+', tr: '+', bl: '+', br: '+', h: '-', v: '|'},
}

/** Cells the title's ` [ ` and ` ] ` take, and where it starts: the corner and one more cell */
const TITLE_FRAME = 6
const TITLE_START = 2

export interface DrawBoxOptions {
    borderStyle?: BorderStyle
    title?: string
    level?: Level
    titleLevel?: Level
    /** Blanks the inside, so what is behind the box doesn't show through */
    fill?: boolean
}

/** Draws a frame `w` by `h` cells with its top-left corner at row `r`, column `c` */
export function drawBox(scene: TuiScene, r: number, c: number, w: number, h: number, options?: DrawBoxOptions): void {
    if (w < 2 || h < 2) return

    const b = BORDER_SETS[options?.borderStyle ?? 'double']
    const level = options?.level ?? 4
    const titleLevel = options?.titleLevel ?? 5

    if (options?.fill ?? true) {
        for (let row = r + 1; row < r + h - 1; row++) {
            for (let col = c + 1; col < c + w - 1; col++) scene.put(row, col, ' ', 1)
        }
    }

    for (let col = c + 1; col < c + w - 1; col++) {
        scene.put(r, col, b.h, level)
        scene.put(r + h - 1, col, b.h, level)
    }
    for (let row = r + 1; row < r + h - 1; row++) {
        scene.put(row, c, b.v, level)
        scene.put(row, c + w - 1, b.v, level)
    }
    scene.put(r, c, b.tl, level)
    scene.put(r, c + w - 1, b.tr, level)
    scene.put(r + h - 1, c, b.bl, level)
    scene.put(r + h - 1, c + w - 1, b.br, level)

    // The title goes last, over the top edge, and is cut to what fits between the corners
    if (options?.title) {
        const room = w - TITLE_START - TITLE_FRAME - 1
        if (room >= 1) scene.write(r, c + TITLE_START, ` [ ${truncateLine(options.title, room)} ] `, titleLevel)
    }
}

export interface TuiBoxWidgetOpts extends TuiWidgetOpts {
    title?: string
    borderStyle?: BorderStyle
    fill?: boolean
}

/** A framed container; its children are placed inside the frame */
export class TuiBoxWidget extends TuiWidget {
    title: string
    borderStyle: BorderStyle
    fill: boolean

    constructor(opts: TuiBoxWidgetOpts) {
        super(opts)
        this.title = opts.title ?? ''
        this.borderStyle = opts.borderStyle ?? 'double'
        this.fill = opts.fill ?? true
        this.level = opts.level ?? 4
        this.inset = 1
    }

    override onRenderSelf(scene: TuiScene): void {
        const {r, c} = this.getGlobalPos()
        drawBox(scene, r, c, this.w, this.h, {
            title: this.title,
            borderStyle: this.borderStyle,
            level: this.isFocused || this.isHovered ? 5 : this.level,
            fill: this.fill,
        })
    }
}
