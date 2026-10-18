import {TuiScene} from '../../tui-engine/core/scene.ts'
import type {PageLayout} from './types.ts'

export {level, ROW_PX} from '../../tui-engine/core/scene.ts'

/**
 * The scene every background draws through: the engine's character grid, which an effect fills with `put` and `flush`
 * draws, plus what the background needs to know about the page in front of it.
 */
export class Scene extends TuiScene {
    layout: PageLayout = {kind: 'chat', sidebarPx: 0, contentPx: 880, coveredRows: 6}

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
