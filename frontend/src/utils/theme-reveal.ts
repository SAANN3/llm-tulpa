/** One step of the reveal in px: two squares of the dot grid behind the pages (12px), so its edge lands on the dots */
const CELL_PX = 24
const DURATION_MS = 800

/**
 * Changes the theme with a reveal from `from`: the page in the new colors grows out of the clicked element as a
 * diamond of grid cells, one cell further in every direction per step, over the old page. Each step is held as it is
 * (the frames are whole cells only), so it reads as a terminal redrawing rather than a smooth wipe. Without the
 * browser's View Transitions, with "reduce motion", or with no element to start from, the theme just changes.
 * `apply` must change the page synchronously: the browser takes its picture of the new page right after it returns.
 */
export function revealTheme(from: Element | null, apply: () => void): void {
    if (!from || !document.startViewTransition || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
        apply()
        return
    }
    const rect = from.getBoundingClientRect()
    const cx = Math.floor((rect.left + rect.width / 2) / CELL_PX)
    const cy = Math.floor((rect.top + rect.height / 2) / CELL_PX)
    const cols = Math.ceil(window.innerWidth / CELL_PX)
    const rows = Math.ceil(window.innerHeight / CELL_PX)
    // Steps until the diamond covers the farthest corner
    const steps = Math.max(cx + cy, cols - cx + cy, cx + rows - cy, cols - cx + rows - cy) + 1
    const frames: Keyframe[] = []
    // `steps(1, end)` holds each frame until the next: the browser never draws a size in between
    for (let k = 0; k <= steps; k++) frames.push({clipPath: diamond(cx, cy, k), easing: 'steps(1, end)'})

    const transition = document.startViewTransition(apply)
    void transition.ready.then(() => {
        document.documentElement.animate(frames, {duration: DURATION_MS, pseudoElement: '::view-transition-new(root)', fill: 'both'})
    }).catch(() => undefined)
}

/** The cells within `k` steps of (cx, cy) along the grid, as the outline of a staircase: one point per corner */
function diamond(cx: number, cy: number, k: number): string {
    const points: [number, number][] = []
    for (let i = 0; i <= k; i++) points.push([cx + i, cy - k + i], [cx + i + 1, cy - k + i])        // top to right
    for (let i = 0; i <= k; i++) points.push([cx + k + 1 - i, cy + i + 1], [cx + k - i, cy + i + 1]) // right to bottom
    for (let i = 0; i < k; i++) points.push([cx - i, cy + k - i], [cx - i - 1, cy + k - i])         // bottom to left
    for (let i = 0; i <= k; i++) points.push([cx - k + i, cy - i], [cx - k + i + 1, cy - i])        // left to top
    return `polygon(${points.map(([x, y]) => `${x * CELL_PX}px ${y * CELL_PX}px`).join(', ')})`
}
