import type {Level, PageLayout, Rgb} from './types.ts'

/** Height of one grid row, px */
export const ROW_PX = 16
const FONT = '13px "JetBrains Mono", ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace'
/** The accent's opacity for levels 1 to 4, and the text color's for level 5 */
const ALPHA = [0.13, 0.20, 0.30, 0.42, 0.50]

export const rgba = (c: Rgb, a: number): string => `rgba(${c[0]},${c[1]},${c[2]},${a})`

// How a theme's accent stands out from its background, as CIELAB lightness (0-100, close to how the eye sees it)
const lightness = (c: Rgb): number => {
    const lin = c.map((v) => { const x = v / 255; return x <= .04045 ? x / 12.92 : ((x + .055) / 1.055) ** 2.4 })
    const y = .2126 * lin[0] + .7152 * lin[1] + .0722 * lin[2]
    return y > 216 / 24389 ? 116 * Math.cbrt(y) - 16 : y * 24389 / 27
}
/** The lightness gap between accent and background the effects were tuned on (the dark themes, around Dusk) */
const TUNED_GAP = 55
/** Thin dark strokes on a light page read weaker than thin light strokes on a dark one, at the same gap */
const LIGHT_PAGE_BOOST = 1.35

/** How much stronger the ink must be on a theme for the effects to show as they do on the themes they were tuned on:
 * 1 on those, more where the accent is closer to the background or the page is light */
export function inkFor(bg: Rgb, accent: Rgb): number {
    const page = lightness(bg), gap = Math.max(8, Math.abs(lightness(accent) - page))
    return Math.min(3, Math.max(1, TUNED_GAP / gap) * (page > 50 ? LIGHT_PAGE_BOOST : 1))
}

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
    /** The theme's own correction (see `inkFor`), set with the colors */
    ink = 1
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
        this.ctx.fillStyle = rgba(this.colors.accent, Math.min(0.9, alpha * this.strength * this.ink))
        const line = '·'.repeat(this.cols)
        for (let r = 0; r < this.rows; r++) this.ctx.fillText(line, 0, r * this.ch)
    }

    /** Draws the grid: one pass per level, each row as a single string with the other levels' cells left blank */
    flush(): void {
        const {cols, rows, chars, levels} = this
        for (let L = 1; L <= 5; L++) {
            this.ctx.fillStyle = rgba(L === 5 ? this.colors.fg : this.colors.accent, Math.min(0.9, ALPHA[L - 1] * this.strength * this.ink * (L === 5 ? 0.8 : 1)))
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
