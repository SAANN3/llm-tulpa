import type {Level, PageLayout, Rgb} from './types.ts'

/** Height of one grid row, px */
export const ROW_PX = 16
const FONT = '13px "JetBrains Mono", ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace'
/** The accent's opacity for levels 1 to 4, and the text color's for level 5 */
const ALPHA = [0.13, 0.20, 0.30, 0.42, 0.50]

export const rgba = (c: Rgb, a: number): string => `rgba(${c[0]},${c[1]},${c[2]},${a})`

/** A computed brightness as a level, rounded and kept in range */
export const level = (n: number): Level => Math.max(0, Math.min(5, Math.round(n))) as Level

/**
 * The one renderer every background draws through: a grid of characters over the whole window. An effect fills the
 * grid with `put` (a character and a level per cell) and `flush` draws it as text, one batch per level, so a frame is
 * a few `fillText` calls however busy it is. It also knows the page's layout and the theme's colors.
 */
export class Scene {
    readonly ctx: CanvasRenderingContext2D
    readonly ch = ROW_PX
    W = 0
    H = 0
    cols = 0
    rows = 0
    /** One character's width, measured from the font */
    cw = 7.8
    /** The grid: a character and a level per cell, row by row */
    chars: string[] = []
    levels: Uint8Array = new Uint8Array(0)
    colors: { bg: Rgb; accent: Rgb; fg: Rgb } = {bg: [0, 0, 0], accent: [0, 0, 0], fg: [0, 0, 0]}
    /** The user's brightness, 1 as tuned */
    strength = 1
    layout: PageLayout = {kind: 'chat', sidebarPx: 0, contentPx: 880, coveredRows: 6}

    constructor(ctx: CanvasRenderingContext2D) {
        this.ctx = ctx
    }

    resize(width: number, height: number, dpr: number): void {
        const canvas = this.ctx.canvas
        this.W = width
        this.H = height
        canvas.width = width * dpr
        canvas.height = height * dpr
        this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
        this.ctx.font = FONT
        this.ctx.textBaseline = 'top'
        this.cw = this.ctx.measureText('M').width
        this.cols = Math.ceil(width / this.cw) + 1
        this.rows = Math.ceil(height / this.ch) + 1
        this.chars = new Array<string>(this.cols * this.rows).fill(' ')
        this.levels = new Uint8Array(this.cols * this.rows)
    }

    // ---- drawing ----

    /** Paints the whole canvas with the theme's background */
    clear(): void {
        this.ctx.fillStyle = rgba(this.colors.bg, 1)
        this.ctx.fillRect(0, 0, this.W, this.H)
    }

    /** The usual start of a frame: the background, the faint base grid of dots, an empty grid to fill */
    begin(): void {
        this.clear()
        this.baseDots(0.10)
        this.clearGrid()
    }

    clearGrid(): void {
        this.chars.fill(' ')
        this.levels.fill(0)
    }

    /** The terminal grid itself: always present, so the field never reads as empty */
    baseDots(alpha: number): void {
        this.ctx.fillStyle = rgba(this.colors.accent, alpha * this.strength)
        const line = '·'.repeat(this.cols)
        for (let r = 0; r < this.rows; r++) this.ctx.fillText(line, 0, r * this.ch)
    }

    /** Draws the grid: one pass per level, each row as a single string with the other levels' cells left blank */
    flush(): void {
        const {cols, rows, chars, levels} = this
        for (let L = 1; L <= 5; L++) {
            this.ctx.fillStyle = rgba(L === 5 ? this.colors.fg : this.colors.accent, Math.min(0.9, ALPHA[L - 1] * this.strength * (L === 5 ? 0.8 : 1)))
            for (let r = 0; r < rows; r++) {
                let s = '', any = false
                const o = r * cols
                for (let c = 0; c < cols; c++) {
                    if (levels[o + c] === L) { s += chars[o + c]; any = true } else s += ' '
                }
                if (any) this.ctx.fillText(s, 0, r * this.ch)
            }
        }
    }

    // ---- writing cells ----

    inside(r: number, c: number): boolean {
        return r >= 0 && r < this.rows && c >= 0 && c < this.cols
    }

    put(r: number, c: number, ch: string, l: Level): void {
        if (!this.inside(r, c)) return
        const i = r * this.cols + c
        this.chars[i] = ch
        this.levels[i] = l
    }

    /** Writes only into an empty cell: for what passes behind everything else */
    putIfEmpty(r: number, c: number, ch: string, l: Level): void {
        if (!this.inside(r, c)) return
        const i = r * this.cols + c
        if (this.levels[i] === 0) { this.chars[i] = ch; this.levels[i] = l }
    }

    /** Writes only over something dimmer */
    putSoft(r: number, c: number, ch: string, l: Level): void {
        if (!this.inside(r, c)) return
        const i = r * this.cols + c
        if (this.levels[i] < l) { this.chars[i] = ch; this.levels[i] = l }
    }

    /** A block of text art at (r0, c0); spaces are left transparent, `levels` gives some characters their own level */
    art(lines: string[], r0: number, c0: number, l: Level, levels?: Record<string, Level>): void {
        lines.forEach((s, i) => {
            for (let k = 0; k < s.length; k++) if (s[k] !== ' ') this.put(r0 + i, c0 + k, s[k], levels?.[s[k]] ?? l)
        })
    }

    // ---- the page in front ----

    /** Rows above the composer, where a scene's ground is */
    visibleRows(): number {
        return this.rows - this.layout.coveredRows
    }

    /** The row a scene stands on */
    floorRow(): number {
        return this.visibleRows() - 1
    }

    /** The first column right of the sidebar */
    areaX(): number {
        return this.layout.kind === 'chat' ? Math.round(this.layout.sidebarPx / this.cw) : 0
    }

    /** The middle column of the space right of the sidebar */
    chatCenterCol(): number {
        return Math.round(((this.layout.sidebarPx + this.W) / 2) / this.cw)
    }

    /** Columns on each side of a centered page's panel */
    sideCols(): number {
        return Math.floor((this.W - this.layout.contentPx) / 2 / this.cw)
    }
}
