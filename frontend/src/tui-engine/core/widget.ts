import {resolveOffset, resolveSize} from './layout.ts'
import type {TuiScene} from './scene.ts'
import type {CellSize, Level, TuiLayoutSpec, TuiPointerEvent, TuiWidgetOpts} from './types.ts'

/**
 * A node of the widget tree. A widget is laid out inside its parent's inner area, drawn into the scene, and found by
 * the pointer through the same tree. Position and size are given as a `spec` (cells, shares, px) and resolved into
 * `x`, `y`, `w` and `h` by `layout`, which the canvas runs before every frame, so a parent's size is always settled
 * before a child is measured against it.
 */
export class TuiWidget {
    /** Offset from the parent's inner top-left corner, in cells; set by `layout` */
    x = 0
    y = 0
    /** Size in cells; set by `layout` */
    w = 0
    h = 0
    spec: TuiLayoutSpec = {}
    level: Level = 3
    visible = true
    clipChildren = false
    focusable = false
    clickable = false
    isHovered = false
    isFocused = false
    /** Cells between the widget's edge and the area its children are placed in (a box's border) */
    inset = 0

    parent: TuiWidget | null = null
    readonly children: TuiWidget[] = []

    private z = 0
    private sorted: TuiWidget[] = []
    private orderDirty = false

    onClick: () => void = () => {}
    onPointerDown(_e: TuiPointerEvent): void {}
    onPointerMove(_e: TuiPointerEvent): void {}
    onPointerUp(_e: TuiPointerEvent): void {}
    onHoverChange(_isHovered: boolean): void {}

    /** A key pressed while the widget is focused; returns whether it used it. Enter and Space press a clickable one. */
    onKey(e: KeyboardEvent): boolean {
        if (!this.clickable || (e.key !== 'Enter' && e.key !== ' ')) return false
        this.onClick()
        return true
    }

    constructor(opts: TuiWidgetOpts = {}) {
        this.apply(opts)
    }

    /** Takes the common options; a React component calls it on every render with its props */
    apply(opts: TuiWidgetOpts): void {
        this.spec = {x: opts.x, y: opts.y, w: opts.w, h: opts.h}
        this.level = opts.level ?? this.level
        this.clipChildren = opts.clipChildren ?? false
        this.visible = opts.visible ?? true
        this.zIndex = opts.zIndex ?? 0
        this.focusable = opts.focusable ?? false
    }

    /** Draws above its siblings with a higher value; equal ones draw in the order they were added */
    get zIndex(): number {
        return this.z
    }

    set zIndex(value: number) {
        if (value === this.z) return
        this.z = value
        if (this.parent) this.parent.orderDirty = true
    }

    add<T extends TuiWidget>(child: T): T {
        child.parent?.remove(child)
        child.parent = this
        this.children.push(child)
        this.orderDirty = true
        return child
    }

    remove(child: TuiWidget): void {
        const index = this.children.indexOf(child)
        if (index === -1) return
        child.parent = null
        this.children.splice(index, 1)
        this.orderDirty = true
    }

    /** The children from back to front. Sorted only when the set or a z-index changed, so a frame allocates nothing. */
    ordered(): readonly TuiWidget[] {
        if (this.orderDirty) {
            this.sorted = [...this.children].sort((a, b) => a.z - b.z)
            this.orderDirty = false
        }
        return this.sorted
    }

    // ---- layout ----

    /** The size of the widget's own content, for a size the spec doesn't give; null for a widget without content, which
     * then takes its parent's whole inner size. `fixedW` is the width when the spec gave one, `availCols` the room. */
    measure(_fixedW: number | null, _availCols: number): { w: number; h: number } | null {
        return null
    }

    /** Resolves this widget's size and position inside a parent inner area of `parentCols` by `parentRows`, then its children's */
    layout(parentCols: number, parentRows: number, cell: CellSize): void {
        const {x, y, w, h} = this.spec
        const fixedW = w === undefined ? null : resolveSize(w, parentCols, cell.cw)
        const fixedH = h === undefined ? null : resolveSize(h, parentRows, cell.ch)
        const content = fixedW === null || fixedH === null ? this.measure(fixedW, parentCols) : null
        this.w = fixedW ?? content?.w ?? parentCols
        this.h = fixedH ?? content?.h ?? parentRows
        // A position that can't be read is the corner, like one that isn't given
        this.x = x === undefined ? 0 : resolveOffset(x, parentCols, this.w, cell.cw) ?? 0
        this.y = y === undefined ? 0 : resolveOffset(y, parentRows, this.h, cell.ch) ?? 0
        this.layoutChildren(cell)
    }

    layoutChildren(cell: CellSize): void {
        const cols = Math.max(0, this.w - 2 * this.inset)
        const rows = Math.max(0, this.h - 2 * this.inset)
        for (const child of this.children) child.layout(cols, rows, cell)
    }

    /** The widget's top-left cell on the canvas */
    getGlobalPos(): { r: number; c: number } {
        let r = this.y
        let c = this.x
        for (let p = this.parent; p; p = p.parent) {
            r += p.y + p.inset
            c += p.x + p.inset
        }
        return {r, c}
    }

    // ---- drawing ----

    /** Draws the widget, then its children from back to front, cut to the inner area if `clipChildren` */
    render(scene: TuiScene, time: number): void {
        if (!this.visible) return
        this.onRenderSelf(scene, time)
        if (this.children.length === 0) return

        if (this.clipChildren) {
            const {r, c} = this.getGlobalPos()
            scene.pushClip({col: c + this.inset, row: r + this.inset, width: Math.max(0, this.w - 2 * this.inset), height: Math.max(0, this.h - 2 * this.inset)})
        }
        for (const child of this.ordered()) child.render(scene, time)
        if (this.clipChildren) scene.popClip()
    }

    /** What the widget draws itself; the children are drawn after it */
    onRenderSelf(_scene: TuiScene, _time: number): void {}

    // ---- the pointer and the keyboard ----

    /** The topmost widget under a canvas cell: the later-drawn child first, a child only where its parent's clip lets it be seen */
    hitTest(col: number, row: number): TuiWidget | null {
        const {r, c} = this.parent ? this.parent.getGlobalPos() : {r: 0, c: 0}
        const inset = this.parent ? this.parent.inset : 0
        return this.hitAt(col, row, r + inset, c + inset)
    }

    private hitAt(col: number, row: number, originR: number, originC: number): TuiWidget | null {
        if (!this.visible) return null
        const r = originR + this.y
        const c = originC + this.x
        const innerR = r + this.inset
        const innerC = c + this.inset

        const seeThrough = !this.clipChildren
            || (row >= innerR && row < r + this.h - this.inset && col >= innerC && col < c + this.w - this.inset)
        if (seeThrough) {
            const kids = this.ordered()
            for (let k = kids.length - 1; k >= 0; k--) {
                const hit = kids[k].hitAt(col, row, innerR, innerC)
                if (hit) return hit
            }
        }
        return row >= r && row < r + this.h && col >= c && col < c + this.w ? this : null
    }

    /** The widgets Tab stops on under this one, in drawing order */
    focusables(out: TuiWidget[] = []): TuiWidget[] {
        for (const child of this.ordered()) {
            if (!child.visible) continue
            if (child.focusable) out.push(child)
            child.focusables(out)
        }
        return out
    }

    /** This widget or the closest parent that takes a click */
    nearestClickable(): TuiWidget | null {
        return this.closest((w) => w.clickable)
    }

    /** This widget or the closest parent that Tab stops on */
    nearestFocusable(): TuiWidget | null {
        return this.closest((w) => w.focusable)
    }

    private closest(test: (w: TuiWidget) => boolean): TuiWidget | null {
        return test(this) ? this : this.parent?.closest(test) ?? null
    }
}
