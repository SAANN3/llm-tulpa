import {hash, noise, rand} from '../engine/random.ts'
import type {Effect, EffectFactory, Level} from '../engine/types.ts'
import {StaticLayer} from '../layers/static-layer.ts'

const BOAT = ['    |\\   ', ' ___|_\\__', ' \\______/']
/** Three layers of mist: speed, size, vertical squash, how thick it must be to show, its level and its height */
const LAYERS: { sp: number; sc: number; ys: number; thr: number; l: Level; off: number }[] = [
    {sp: .03, sc: .03, ys: .5, thr: .5, l: 1, off: -.05}, {sp: .05, sc: .045, ys: .6, thr: .48, l: 1, off: .02}, {sp: .085, sc: .06, ys: .7, thr: .47, l: 1, off: .08},
]

/** Fog over a lake: three layers of mist drifting at different speeds, a dark tree line in the distance and a boat
 * with a warm lantern that shows and sinks into the haze */
export const fog = (): EffectFactory => (s): Effect => {
    const fixed = new StaticLayer(s)
    let vis = 0, hz = 0, a0 = 0, aw = 0, seed = 0, boat = 0
    return {
        init() {
            vis = s.visibleRows(); hz = Math.round(.58 * vis); a0 = s.areaX(); aw = s.cols - a0; seed = rand() * 400; fixed.reset()
            for (let c = 0; c < s.cols; c++) {
                const r0 = Math.round(hz - 2 - (noise(c * .05 + seed, 1, 1) - .3) * 5)
                for (let r = r0; r < hz; r++) fixed.put(r, c, ':', 1)
                fixed.put(r0, c, '_', 2)
                if (hash(c, 1, 7, Math.floor(seed)) < .13) {
                    const h = 2 + Math.floor(hash(c, 2, 7, Math.floor(seed)) * 3)
                    for (let k = 0; k < h; k++) fixed.put(r0 - k, c, k === h - 1 ? '^' : '|', 3)
                }
            }
            boat = a0 + aw * .5
        },
        draw(d, t) {
            const {cols, rows} = s
            s.begin()
            fixed.blit()
            boat += .25 * d
            if (boat > cols + 10) boat = a0 - 10
            const bx = Math.round(boat), by = hz + 6 + Math.round(Math.sin(t * .5) * .6)
            s.art(BOAT, by, bx, 3)
            const pulse = .5 + .5 * Math.sin(t * .8)
            for (let dr = -3; dr <= 3; dr++) for (let dc = -8; dc <= 8; dc++) {
                const dd = (dc / 8) ** 2 + (dr / 3) ** 2
                if (dd < 1 && hash(dc, dr, 5, 3) < (1 - dd) * .5) s.putIfEmpty(by - 2 + dr, bx + 4 + dc, '.', dd < .35 ? 2 : 1)
            }
            s.put(by - 2, bx + 4, '*', pulse > .5 ? 4 : 3)
            LAYERS.forEach((L, k) => {
                const bc = hz + L.off * vis, bw = vis * (.17 + k * .03)
                for (let r = 0; r < rows; r++) {
                    const wg = Math.exp(-(((r - bc) / bw) ** 2))
                    if (wg < .1) continue
                    for (let c = 0; c < cols; c++) {
                        const n = noise(c * L.sc * 1.7 + t * L.sp + k * 40, r * L.ys * 1.4 + k * 7, t * .02) * wg * 1.3
                        if (n < L.thr || hash(c, r, k, 4) < .28) continue
                        if (s.levels[r * cols + c] >= 3) continue
                        s.put(r, c, n > .72 ? '~' : n > .58 ? ':' : '.', k === 2 && n > .7 ? 2 : 1)
                    }
                }
            })
            s.flush()
        },
    }
}
