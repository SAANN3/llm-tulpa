import {inkFor, rgba} from './color.ts'
import {cellWidth, graphemes} from './text-cells.ts'
import type {Level, ThemeColors, TuiRect} from './types.ts'

/** Height of one grid row, px */
export const ROW_PX = 16
const FONT = '13px "JetBrains Mono", ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace'
/** The accent's opacity for levels 1 to 4, and the text color's for level 5 */
const ALPHA = [0.13, 0.20, 0.30, 0.42, 0.50]

/** A computed brightness as a level, rounded and kept in range */
export const level = (n: number): Level => Math.max(0, Math.min(5, Math.round(n))) as Level

/** The last code point of ASCII: the font has these at the width of a cell, so they are not measured */
const ASCII_END = 0x7e
/** A symbol whose width is this far (px) from a cell's is drawn at its own cell rather than in the row */
const ADVANCE_TOLERANCE = 0.01
/** Beyond this (px) a symbol is also squeezed or stretched to its cell: closer than that, moving it is enough */
const SQUEEZE_FROM = 0.3

/**
 * The one renderer the animated backgrounds and the interface widgets both draw through: a grid of characters over a
 * canvas. A drawing step fills the grid with `put` (a character and a level per cell) and `flush` draws it as text, one
 * batch per level, so a frame is a few `fillText` calls however busy it is. It also knows the theme's colors.
 *
 * A symbol that is two cells wide (an emoji, a CJK character) sits in the first cell of its pair, with an empty string
 * in the second: `flush` draws it on its own at its cell, so it can't push the rest of the row out of line.
 */
export class TuiScene {
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
    colors: ThemeColors = {bg: [0, 0, 0], accent: [0, 0, 0], fg: [0, 0, 0]}
    /** The user's brightness, 1 as tuned */
    strength = 1
    /** The theme's own correction (see `inkFor`), set with the colors */
    ink = 1
    /** Pixels the text is drawn lower inside its row, for a canvas whose glyphs would otherwise be cut off at its top edge */
    textShiftY = 0
    /** Leaves the canvas clear instead of painting it in the theme's background, so what is behind it shows through */
    transparent = false
    /** The area drawing is limited to: the innermost of the pushed clips, none when nothing is pushed */
    private clip: TuiRect | null = null
    private clips: TuiRect[] = []
    /** Whether a wide symbol was written since the grid was cleared; keeps the checks for one off the common path */
    private hasWide = false
    /** The width in px of a symbol in the font that draws it, once measured: see `flush` */
    private advances = new Map<string, number>()

    constructor(ctx: CanvasRenderingContext2D) {
        this.ctx = ctx
    }

    /** The whole cells that fit the canvas; `cols` and `rows` also count the part-cell at the right and bottom edge */
    get fitCols(): number {
        return Math.floor(this.W / this.cw)
    }

    get fitRows(): number {
        return Math.floor(this.H / this.ch)
    }

    setColors(colors: ThemeColors): void {
        this.colors = colors
        this.ink = inkFor(colors.bg, colors.accent)
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
        this.advances.clear()
        this.cols = Math.ceil(width / this.cw) + 1
        this.rows = Math.ceil(height / this.ch) + 1
        this.chars = new Array<string>(this.cols * this.rows).fill(' ')
        this.levels = new Uint8Array(this.cols * this.rows)
    }

    // ---- drawing ----

    /** Paints the whole canvas with the theme's background, or empties it when `transparent` */
    clear(): void {
        if (this.transparent) {
            this.ctx.clearRect(0, 0, this.W, this.H)
            return
        }
        this.ctx.fillStyle = rgba(this.colors.bg, 1)
        this.ctx.fillRect(0, 0, this.W, this.H)
    }

    /** The usual start of a frame: the background, the faint base grid of dots (`null` for none), an empty grid to fill */
    begin(dots: number | null = 0.10): void {
        this.clips = []
        this.clip = null
        this.clear()
        if (dots !== null) this.baseDots(dots)
        this.clearGrid()
    }

    clearGrid(): void {
        this.chars.fill(' ')
        this.levels.fill(0)
        this.hasWide = false
    }

    /** The terminal grid itself: always present, so the field never reads as empty */
    baseDots(alpha: number): void {
        this.ctx.fillStyle = rgba(this.colors.accent, Math.min(0.9, alpha * this.strength * this.ink))
        const line = '·'.repeat(this.cols)
        for (let r = 0; r < this.rows; r++) this.ctx.fillText(line, 0, r * this.ch + this.textShiftY)
    }

    /**
     * A symbol outside the ASCII range that the font draws at some width other than a cell's: the font has no glyph for it
     * (box drawing and braille are not in the subsets the page loads) and another font, which differs by device, draws it.
     * In a row string every such symbol would push the rest of the row out of line.
     */
    private misfit(ch: string): number | null {
        if (ch === '' || ch.charCodeAt(0) <= ASCII_END) return null
        let advance = this.advances.get(ch)
        if (advance === undefined) {
            advance = this.ctx.measureText(ch).width
            this.advances.set(ch, advance)
        }
        return Math.abs(advance - this.cw) > ADVANCE_TOLERANCE ? advance : null
    }

    /**
     * Draws the grid: one pass per level, each row as a single string with the other levels' cells left blank. The wide
     * symbols and the ones `misfit` finds are drawn on their own at their cell, the latter squeezed or stretched to it.
     */
    flush(): void {
        const {cols, rows, chars, levels, hasWide} = this
        for (let L = 1; L <= 5; L++) {
            this.ctx.fillStyle = this.levelStyle(L)
            for (let r = 0; r < rows; r++) {
                let s = '', any = false
                let wide: number[] | null = null
                let fitted: number[] | null = null
                const o = r * cols
                for (let c = 0; c < cols; c++) {
                    if (levels[o + c] !== L) { s += ' '; continue }
                    any = true
                    if (hasWide && chars[o + c] === '') s += ' '
                    else if (hasWide && c + 1 < cols && chars[o + c + 1] === '') { (wide ??= []).push(c); s += ' ' }
                    else if (this.misfit(chars[o + c]) != null) { (fitted ??= []).push(c); s += ' ' }
                    else s += chars[o + c]
                }
                if (any) this.ctx.fillText(s, 0, r * this.ch + this.textShiftY)
                if (wide) for (const c of wide) this.ctx.fillText(chars[o + c], c * this.cw, r * this.ch + this.textShiftY)
                if (fitted) for (const c of fitted) this.fillFitted(chars[o + c], c * this.cw, r * this.ch + this.textShiftY)
            }
        }
    }

    private fillFitted(ch: string, x: number, y: number): void {
        const advance = this.advances.get(ch) ?? this.cw
        if (Math.abs(advance - this.cw) <= SQUEEZE_FROM) {
            this.ctx.fillText(ch, x, y)
            return
        }
        this.ctx.save()
        this.ctx.translate(x, y)
        this.ctx.scale(this.cw / advance, 1)
        this.ctx.fillText(ch, 0, 0)
        this.ctx.restore()
    }

    /** The fill style of a level: the accent at a rising opacity for 1 to 4, the text color for 5 */
    private levelStyle(l: number): string {
        return rgba(l === 5 ? this.colors.fg : this.colors.accent, Math.min(0.9, ALPHA[l - 1] * this.strength * this.ink * (l === 5 ? 0.8 : 1)))
    }

    // ---- clipping ----

    /** Limits drawing to `rect` (cut down to the clip already in force) until the matching `popClip` */
    pushClip(rect: TuiRect): void {
        const cur = this.clip
        if (cur) {
            const col = Math.max(cur.col, rect.col), row = Math.max(cur.row, rect.row)
            rect = {
                col, row,
                width: Math.max(0, Math.min(cur.col + cur.width, rect.col + rect.width) - col),
                height: Math.max(0, Math.min(cur.row + cur.height, rect.row + rect.height) - row),
            }
        }
        this.clips.push(rect)
        this.clip = rect
    }

    popClip(): void {
        this.clips.pop()
        this.clip = this.clips[this.clips.length - 1] ?? null
    }

    // ---- writing cells ----

    inside(r: number, c: number): boolean {
        if (r < 0 || r >= this.rows || c < 0 || c >= this.cols) return false
        const k = this.clip
        return !k || (r >= k.row && r < k.row + k.height && c >= k.col && c < k.col + k.width)
    }

    put(r: number, c: number, ch: string, l: Level): void {
        if (!this.inside(r, c)) return
        const i = r * this.cols + c
        if (this.hasWide) this.splitGuard(i, c)
        this.chars[i] = ch
        this.levels[i] = l
    }

    /** Writes only into an empty cell: for what passes behind everything else */
    putIfEmpty(r: number, c: number, ch: string, l: Level): void {
        if (!this.inside(r, c)) return
        const i = r * this.cols + c
        if (this.levels[i] === 0) {
            if (this.hasWide) this.splitGuard(i, c)
            this.chars[i] = ch
            this.levels[i] = l
        }
    }

    /** Writes only over something dimmer */
    putSoft(r: number, c: number, ch: string, l: Level): void {
        if (!this.inside(r, c)) return
        const i = r * this.cols + c
        if (this.levels[i] < l) {
            if (this.hasWide) this.splitGuard(i, c)
            this.chars[i] = ch
            this.levels[i] = l
        }
    }

    /** Writes one symbol, which fills one or two cells; a wide one is skipped whole if either of its cells is out of reach */
    putGlyph(r: number, c: number, g: string, l: Level): void {
        if (cellWidth(g) === 1) return this.put(r, c, g, l)
        if (!this.inside(r, c) || !this.inside(r, c + 1)) return
        const i = r * this.cols + c
        this.hasWide = true
        this.splitGuard(i, c)
        this.splitGuard(i + 1, c + 1)
        this.chars[i] = g
        this.levels[i] = l
        this.chars[i + 1] = ''
        this.levels[i + 1] = l
    }

    /** Writes a text along row `r` from column `c`; returns the cells it filled */
    write(r: number, c: number, text: string, l: Level = 5): number {
        let x = 0
        for (const g of graphemes(text)) {
            this.putGlyph(r, c + x, g, l)
            x += cellWidth(g)
        }
        return x
    }

    /** A block of text art at (r0, c0); spaces are left transparent, `levels` gives some characters their own level */
    art(lines: string[], r0: number, c0: number, l: Level, levels?: Record<string, Level>): void {
        lines.forEach((s, i) => {
            for (let k = 0; k < s.length; k++) if (s[k] !== ' ') this.put(r0 + i, c0 + k, s[k], levels?.[s[k]] ?? l)
        })
    }

    /** Writing over one half of a wide symbol would leave the other half drawn alone, two cells wide in one: blank it */
    private splitGuard(i: number, c: number): void {
        if (this.chars[i] === '') {
            if (c > 0) this.chars[i - 1] = ' '
        } else if (c + 1 < this.cols && this.chars[i + 1] === '') {
            this.chars[i + 1] = ' '
        }
    }
}
