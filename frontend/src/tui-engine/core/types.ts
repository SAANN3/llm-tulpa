/** A grid cell's brightness: 0 is empty, 1 to 4 are the accent at rising opacity, 5 is the text color */
export type Level = 0 | 1 | 2 | 3 | 4 | 5

export type Rgb = [number, number, number]

/** The three theme colors a scene draws with, read from the page's design tokens */
export interface ThemeColors {
    bg: Rgb
    accent: Rgb
    fg: Rgb
}

/** A rectangle in grid cells */
export interface TuiRect {
    col: number
    row: number
    width: number
    height: number
}

/** The size of one grid cell in px, which `px` lengths are measured in */
export interface CellSize {
    cw: number
    ch: number
}

export type BorderStyle = 'single' | 'double' | 'rounded' | 'heavy' | 'ascii'

export interface BorderGlyphs {
    tl: string
    tr: string
    bl: string
    br: string
    h: string
    v: string
}

/** A position or size: cells (`12`), a share of the parent (`'50%'`, `'100% - 2'`), px (`'120px'`) or, for a position
 * only, `'center'` */
export type TuiDimension = number | string

/** Where a widget goes inside its parent's inner area, and how big it is. A missing size is the widget's content
 * size, or the parent's whole inner size for a widget without content (a box). */
export interface TuiLayoutSpec {
    x?: TuiDimension
    y?: TuiDimension
    w?: TuiDimension
    h?: TuiDimension
}

export interface TuiWidgetOpts extends TuiLayoutSpec {
    clipChildren?: boolean
    level?: Level
    visible?: boolean
    zIndex?: number
    /** Whether Tab stops on it; a widget with a click handler is focusable unless this says otherwise */
    focusable?: boolean
}

/** What every React component of the engine takes */
export interface TuiBaseProps extends TuiWidgetOpts {
    onClick?: () => void
}

/** A pointer press, move or release, handed up the widget tree from the widget under the pointer */
export interface TuiPointerEvent {
    /** Column and row on the canvas */
    col: number
    row: number
    /** Column and row relative to the widget the event is handed to */
    localCol: number
    localRow: number
    originalEvent: PointerEvent
    /** Keeps the event from going on to the widget's parent */
    stopPropagation: () => void
}

/** How often the canvas draws */
export const TuiRenderMode = {
    /** Every animation frame, at the canvas's `fps`: for anything that moves on its own */
    Continuous: 'continuous',
    /** Only after something asked for it (a change of props, the pointer, `requestRedraw`) */
    OnDemand: 'on-demand',
} as const

export type TuiRenderMode = (typeof TuiRenderMode)[keyof typeof TuiRenderMode]
