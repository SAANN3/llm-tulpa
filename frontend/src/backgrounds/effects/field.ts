import {hash, noise, rand} from '../engine/random.ts'
import type {Effect, EffectFactory} from '../engine/types.ts'
import {CloudsLayer} from '../layers/clouds.ts'
import {StaticLayer} from '../layers/static-layer.ts'

interface Bird { x: number; y: number; ph: number }
interface Flower { r: number; c: number; k: number }

/** A field by day: a wave of wind running through the grass, a winding path, a windmill turning slowly on the hill,
 * clouds and a flock of birds */
export const field = (): EffectFactory => (s): Effect => {
    const fixed = new StaticLayer(s)
    let vis = 0, a0 = 0, aw = 0, hz = 0, path: Uint8Array = new Uint8Array(0), ridgeA: number[] = [], seed = 0
    let mill: { c: number; base: number; ang: number } = {c: 0, base: 0, ang: 0}, clouds: CloudsLayer | null = null, birds: Bird[] = [], flowers: Flower[] = []
    return {
        init() {
            const {cols, rows} = s
            vis = s.visibleRows(); a0 = s.areaX(); aw = cols - a0; seed = rand() * 500; hz = Math.round(.56 * vis)
            fixed.reset(); path = new Uint8Array(cols * rows)
            const rg = (c: number, base: number, amp: number, fq: number, k: number) => (base + amp * ((noise(c * fq + seed, k * 5.1, 3) - .5) * 1.8 + (noise(c * fq * 2.7 + seed, k * 2.3, 4) - .5) * .6)) * vis
            ridgeA = Array.from({length: cols}, (_, c) => rg(c, .44, .045, .04, 1))
            const ridgeB = Array.from({length: cols}, (_, c) => rg(c, .52, .035, .06, 2))
            const ridges: [number[], string][] = [[ridgeA, '.'], [ridgeB, ':']]
            ridges.forEach(([R, ch]) => {
                for (let c = 0; c < cols; c++) {
                    const r0 = Math.round(R[c]), sl = Math.round(R[Math.min(cols - 1, c + 1)]) - Math.round(R[Math.max(0, c - 1)])
                    for (let r = r0; r < rows; r++) fixed.put(r, c, r === r0 ? (sl < 0 ? '/' : sl > 0 ? '\\' : '_') : ch, r === r0 ? 2 : 1)
                }
            })
            for (let r = hz; r < rows; r++) for (let c = 0; c < cols; c++) fixed.put(r, c, ' ', 1)   // the field itself is drawn live
            // path
            const pc = a0 + aw * .45
            for (let r = hz; r < rows; r++) {
                const u = (r - hz) / Math.max(1, rows - hz), cen = pc + Math.sin(r * .17) * (2 + 10 * u), hw = 1 + 12 * Math.pow(u, 1.3)
                for (let c = Math.floor(cen - hw - 1); c <= Math.ceil(cen + hw + 1); c++) {
                    if (c < 0 || c >= cols) continue
                    const dd = Math.abs(c - cen)
                    if (dd <= hw) { path[r * cols + c] = 1; fixed.put(r, c, hash(c, r, 2, 5) < .08 ? ':' : ' ', 1) }
                    else if (dd <= hw + 1) fixed.put(r, c, ':', 2)
                }
            }
            // trees on the far ridge
            for (let i = 0; i < Math.max(3, Math.round(aw / 30)); i++) {
                const c = a0 + 4 + Math.floor(hash(i, 1, 1, Math.floor(seed)) * (aw - 8)), base = Math.round(ridgeB[c]) + 2, rx = 4 + Math.floor(hash(i, 2, 1, Math.floor(seed)) * 3)
                for (let dr = -2; dr <= 2; dr++) for (let dc = -rx; dc <= rx; dc++) {
                    const dd = (dr / 2.4) ** 2 + (dc / rx) ** 2
                    if (dd <= 1) fixed.put(base - 3 + dr, c + dc, ['&', '@', '%', '*'][Math.floor(hash(dc, dr, i, 3) * 4)], dd > .7 ? 1 : 2)
                }
                fixed.put(base - 1, c, '|', 3); fixed.put(base, c, '|', 3)
            }
            const wc = a0 + Math.round(aw * .74)
            mill = {c: wc, base: Math.round(ridgeA[wc]), ang: rand() * 6}
            flowers = Array.from({length: Math.round(cols * rows * .006)}, () => ({r: hz + 1 + Math.floor(rand() * (vis - hz)), c: Math.floor(rand() * cols), k: rand()}))
            clouds = new CloudsLayer(s, {n: 4, kinds: ['cu', 'cu', 'st'], z: [.2, .7], maxL: 3, big: .62, y: () => [1, Math.max(3, vis * .3)]})
            clouds.init()
            birds = Array.from({length: 5}, (_, i) => ({x: a0 + aw * .1 - i * 4, y: Math.round(.2 * vis) + (i % 2 ? 1 : -1) * Math.ceil(i / 2) * 1.5, ph: rand() * 3}))
        },
        draw(d, t) {
            const {cols, rows} = s
            s.begin()
            clouds?.draw(d, t); fixed.blit()
            // windmill
            {
                const H = 5, cx = mill.c, base = mill.base + 1
                for (let i = 0; i < H; i++) {
                    const hw = Math.round(2.4 - i * .3)
                    s.put(base - i, cx - hw, '/', 3); s.put(base - i, cx + hw, '\\', 3)
                    if (i > 0 && i < H - 1 && i % 2 === 0) s.put(base - i, cx, '+', 2)
                }
                s.put(base - H, cx, 'o', 4)
                mill.ang += .35 * d
                const hy = base - H
                for (let k = 0; k < 4; k++) {
                    const a = mill.ang + k * Math.PI / 2, L = 4, n = L * 2
                    for (let st = 1; st <= n; st++) {
                        const dx = Math.cos(a) * L * st / n, dy = Math.sin(a) * L * st / n, sl = Math.abs(dy / (dx / 2.05 || .001))
                        s.put(Math.round(hy + dy), Math.round(cx + dx * 2.05), sl > 2.4 ? '|' : sl < .42 ? '-' : (dx * dy > 0 ? '\\' : '/'), 3)
                    }
                }
            }
            // grass waves
            for (let r = hz; r < rows; r++) {
                const near = (r - hz) / Math.max(1, rows - hz)
                for (let c = 0; c < cols; c++) {
                    const i = r * cols + c
                    if (path[i]) continue
                    if (hash(c, r, 6, 11) > .26 + near * .3) continue
                    const w = Math.sin(c * .22 + r * .35 - t * 1.3) * .6 + Math.sin(c * .09 - t * .7) * .4
                    s.put(r, c, w > .5 ? '/' : (hash(r, c, 8, 1) < .5 ? '"' : ','), w > .5 ? 2 : 1)
                }
            }
            for (const f of flowers) if (!path[f.r * cols + f.c]) s.put(f.r, f.c, f.k < .6 ? 'o' : '*', f.k < .6 ? 2 : 3)
            for (const b of birds) {
                b.x += 1.5 * d
                if (b.x > cols + 6) b.x = a0 - 8
                s.put(Math.round(b.y + Math.sin(t * .4 + b.ph) * .8), Math.floor(b.x), Math.floor(t * 2.4 + b.ph * 3) % 2 ? 'v' : '^', 3)
            }
            s.flush()
        },
    }
}

