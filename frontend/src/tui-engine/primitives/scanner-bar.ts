import type {TuiScene} from '../core/scene.ts'
import type {BorderStyle, Level, TuiWidgetOpts} from '../core/types.ts'
import {TuiWidget} from '../core/widget.ts'
import {BORDER_SETS} from './box.ts'

export type ScannerFillMode = 'code' | 'hash'
export type ScannerRevealMode = 'empty' | 'color'

/** What a scanner bar is set with; the widget and its React component take the same options */
export interface ScannerBarOptions {
    /** Progress from 0 to 100. Without it the bar sweeps back and forth, for work with no known end. */
    value?: number
    /** Sweeps even with a `value` given; by default a bar sweeps exactly when it has no `value` */
    indeterminate?: boolean
    /** What the scanned part is made of: a stream of letters and digits (`code`) or `#` (`hash`) */
    fillMode?: ScannerFillMode
    /** What is ahead of the scan head: nothing (`empty`) or the same glyphs dimmed (`color`) */
    revealMode?: ScannerRevealMode
    /** The percentage after the bar, when there is a `value` and the room for it */
    showPercent?: boolean
    /** Sweeps per second while sweeping (a sweep is there and back) */
    speed?: number
    /** The frame of a bar three rows or more high */
    borderStyle?: BorderStyle
    /** Seeds the stream of glyphs and the brightness of each; random for each bar when not given */
    seed?: number
}

export interface TuiScannerBarOpts extends TuiWidgetOpts, ScannerBarOptions {}

const CODE_CHARS = 'abcdefghijklmnopqrstuvwxyz0123456789%#@*+=~'
const HASH_CHAR = '#'
/** The levels the scanned part is drawn at, one per cell, so it reads as noise */
const TRAIL_LEVELS: Level[] = [2, 3, 4]
/** The scan head and the percentage */
const HEAD_LEVEL: Level = 5
/** The two columns of dots either side of the head's line in a framed bar */
const HEAD_FLANK_LEVEL: Level = 3
/** The glyphs ahead of the head when they are shown dimmed rather than left out */
const AHEAD_LEVEL: Level = 1
/** A space and `100%`: what the percentage takes right of the bar */
const LABEL_CELLS = 5
/** The narrowest bar that is still one: its two ends and two cells between them */
const MIN_BAR_CELLS = 4
/** Rows from which a bar has a frame (top, bottom and at least one row between); fewer are rows between brackets */
const MIN_FRAMED_ROWS = 3
/** Sweeps per second when `speed` isn't given: one there and back takes four seconds */
const DEFAULT_SPEED = 0.25

/**
 * A progress bar in the manner of a scanner: what the head has passed is a stream of glyphs, the head is a bright mark.
 * One or two rows high it is that many rows between `[` and `]`, the head an `I` in each; from three rows it is a
 * frame with the head drawn as a line across it. With a `value` it shows that progress, without one it sweeps back and forth.
 */
export class TuiScannerBarWidget extends TuiWidget {
    value?: number
    indeterminate = true
    fillMode: ScannerFillMode = 'code'
    revealMode: ScannerRevealMode = 'empty'
    showPercent = true
    speed = DEFAULT_SPEED
    borderStyle: BorderStyle = 'single'
    seed = Math.floor(Math.random() * 0x7fffffff)

    constructor(opts: TuiScannerBarOpts = {}) {
        super(opts)
        this.level = opts.level ?? 4
        this.configure(opts)
    }

    /** Takes the bar's own options, each falling back to its default when missing */
    configure(opts: ScannerBarOptions): void {
        this.value = opts.value
        this.indeterminate = opts.indeterminate ?? opts.value == null
        this.fillMode = opts.fillMode ?? 'code'
        this.revealMode = opts.revealMode ?? 'empty'
        this.showPercent = opts.showPercent ?? true
        this.speed = opts.speed ?? DEFAULT_SPEED
        this.borderStyle = opts.borderStyle ?? 'single'
        if (opts.seed != null) this.seed = opts.seed
    }

    /** A hash of the cell and the seed, so the same cell always gets the same glyph and brightness */
    private hash(r: number, c: number, salt: number): number {
        let h = (r * 0x85ebca6b) ^ (c * 0xc2b2ae35) ^ this.seed ^ salt
        h ^= h >>> 16
        h = Math.imul(h, 0x7feb352d)
        h ^= h >>> 15
        return Math.abs(h)
    }

    private progress(time: number): number {
        if (this.indeterminate || this.value == null) return (Math.sin(2 * Math.PI * this.speed * time) + 1) / 2
        return Math.max(0, Math.min(1, this.value / 100))
    }

    /** One cell of the bar, by its place relative to the head */
    private drawCell(scene: TuiScene, r: number, c: number, passed: boolean, ahead: boolean): void {
        const glyph = this.fillMode === 'code' ? CODE_CHARS[this.hash(r, c, 1) % CODE_CHARS.length] : HASH_CHAR
        if (passed) scene.put(r, c, glyph, TRAIL_LEVELS[this.hash(r, c, 2) % TRAIL_LEVELS.length])
        else if (ahead && this.revealMode === 'color') scene.put(r, c, glyph, AHEAD_LEVEL)
        else if (ahead) scene.put(r, c, ' ', 0)
    }

    override onRenderSelf(scene: TuiScene, time: number): void {
        const {r, c} = this.getGlobalPos()
        const progress = this.progress(time)
        const showLabel = this.showPercent && !this.indeterminate && this.w - LABEL_CELLS >= MIN_BAR_CELLS
        const barW = showLabel ? this.w - LABEL_CELLS : this.w
        if (barW < MIN_BAR_CELLS || this.h < 1) return

        const framed = this.h >= MIN_FRAMED_ROWS
        const innerW = barW - 2
        const head = Math.min(innerW - 1, Math.floor(progress * innerW))
        // A framed bar's cells are the rows between the frame; a bar of one or two rows is cells in all of them
        const firstRow = framed ? r + 1 : r
        const lastRow = framed ? r + this.h - 2 : r + this.h - 1

        if (framed) this.drawFrame(scene, r, c, barW)
        else {
            for (let row = firstRow; row <= lastRow; row++) {
                scene.put(row, c, '[', this.level)
                scene.put(row, c + barW - 1, ']', this.level)
            }
        }

        for (let row = firstRow; row <= lastRow; row++) {
            for (let i = 0; i < innerW; i++) {
                if (i !== head) this.drawCell(scene, row, c + 1 + i, i < head, i > head)
            }
        }

        if (framed) this.drawHead(scene, r, c + 1 + head, head, innerW)
        else for (let row = firstRow; row <= lastRow; row++) scene.put(row, c + 1 + head, 'I', HEAD_LEVEL)

        if (showLabel) scene.write(r + Math.floor(this.h / 2), c + barW + 1, `${Math.round(progress * 100)}`.padStart(3) + '%', HEAD_LEVEL)
    }

    private drawFrame(scene: TuiScene, r: number, c: number, barW: number): void {
        const b = BORDER_SETS[this.borderStyle]
        const bottom = r + this.h - 1
        scene.put(r, c, b.tl, this.level)
        scene.put(r, c + barW - 1, b.tr, this.level)
        scene.put(bottom, c, b.bl, this.level)
        scene.put(bottom, c + barW - 1, b.br, this.level)
        for (let x = 1; x < barW - 1; x++) {
            scene.put(r, c + x, b.h, this.level)
            scene.put(bottom, c + x, b.h, this.level)
        }
        for (let y = 1; y < this.h - 1; y++) {
            scene.put(r + y, c, b.v, this.level)
            scene.put(r + y, c + barW - 1, b.v, this.level)
        }
    }

    /** The head of a framed bar: a line through the rows between, with a cap on the frame above and below it */
    private drawHead(scene: TuiScene, r: number, col: number, head: number, innerW: number): void {
        const bottom = r + this.h - 1
        const flankLeft = head > 0
        const flankRight = head < innerW - 1
        for (let row = r + 1; row < bottom; row++) {
            if (flankLeft) scene.put(row, col - 1, '.', HEAD_FLANK_LEVEL)
            scene.put(row, col, '|', HEAD_LEVEL)
            if (flankRight) scene.put(row, col + 1, '.', HEAD_FLANK_LEVEL)
        }
        for (const [edge, cap] of [[r, '╤'], [bottom, '╧']] as const) {
            if (flankLeft) scene.put(edge, col - 1, '─', HEAD_LEVEL)
            scene.put(edge, col, cap, HEAD_LEVEL)
            if (flankRight) scene.put(edge, col + 1, '─', HEAD_LEVEL)
        }
    }
}
