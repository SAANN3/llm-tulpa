import {clamp, hash, rand} from '../engine/random.ts'
import type {Effect, EffectFactory} from '../engine/types.ts'

interface Blob { x: number; r: number; py: number; per: number; wx: number; wper: number; ax: number; y0: number; ay: number }

const RAMP = ['.', ':', '-', '=', '+', 'o']

/** A lava lamp: big soft blobs of wax slowly rising, merging and sinking, a little warmer at the bottom */
export const lava = (): EffectFactory => (s): Effect => {
    let blobs: Blob[] = []
    return {
        init() {
            const {cols, rows} = s
            const n = Math.max(9, Math.round(cols / 15))
            blobs = Array.from({length: n}, () => ({x: rand() * cols, r: rows * (.04 + rand() * .06), py: rand() * 6.28, per: 24 + rand() * 30, wx: rand() * 6.28, wper: 40 + rand() * 40, ax: 4 + rand() * 8, y0: rows * (.3 + rand() * .4), ay: rows * (.2 + rand() * .18)}))
        },
        draw(_d, t) {
            const {cols, rows} = s
            s.begin()
            const B = blobs.map(b => ({x: b.x + Math.sin(t / b.wper * 6.283 + b.wx) * b.ax, y: b.y0 + Math.sin(t / b.per * 6.283 + b.py) * b.ay, r2: b.r * b.r}))
            for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) {
                // a metaball field; columns count for half a row, since a cell is about twice as tall as it is wide
                let f = 0
                for (const b of B) { const dx = (c - b.x) * .488, dy = r - b.y; f += b.r2 / (dx * dx + dy * dy + 1) }
                f += (r / rows) * .15
                if (f < .8) { if (r > rows - 5 && hash(c, r, 3, 2) < .25 * (r - rows + 5) / 5) s.put(r, c, '.', 1); continue }
                const k = clamp(Math.floor((f - .8) * 3), 0, 5)
                s.put(r, c, RAMP[k], k < 2 ? 1 : k < 5 ? 2 : 3)
            }
            s.flush()
        },
    }
}
