import {hash, rand} from '../engine/random.ts'
import type {Effect, EffectFactory} from '../engine/types.ts'
import {CloudsLayer} from '../layers/clouds.ts'
import {StaticLayer} from '../layers/static-layer.ts'

interface Window { r: number; c: number; id: number; layer: number }
interface Drop { x: number; y: number; v: number }
interface GlassDrop { x: number; y: number; wait: number; sl: number }
interface Ripple { x: number; t0: number }

/** A rainy evening over a city: two layers of houses, windows lighting up and going dark, a street lamp with a halo,
 * ripples in the puddles and drops sliding down the glass */
export const rainyCity = (): EffectFactory => (s): Effect => {
    const fixed = new StaticLayer(s)
    let vis = 0, gr = 0, a0 = 0, aw = 0, wins: Window[] = [], rain: Drop[] = [], glass: GlassDrop[] = [], rip: Ripple[] = []
    let trail: Float32Array = new Float32Array(0), clouds: CloudsLayer | null = null, nextRip = 0, seed = 0
    return {
        init() {
            const {cols, rows} = s
            vis = s.visibleRows(); gr = vis - 1; a0 = s.areaX(); aw = cols - a0; seed = Math.floor(rand() * 1e6)
            fixed.reset(); wins = []
            let id = 0
            for (const layer of [0, 1]) {
                let c = layer ? 0 : -4
                while (c < cols) {
                    const w = 8 + Math.floor(rand() * 8), h = Math.round(vis * (layer ? .15 + rand() * .22 : .27 + rand() * .25)), top = gr - h
                    for (let r = top; r <= gr + 1; r++) for (let k = 0; k < w; k++) {
                        const roof = r === top, wall = layer && (k === 0 || k === w - 1)
                        fixed.put(r, c + k, roof ? '_' : wall ? '|' : (layer ? ':' : '.'), roof || wall ? 2 : 1)
                    }
                    for (let r = top + 2; r < gr - 1; r += 3) for (let k = 2; k < w - 3; k += 4) wins.push({r, c: c + k, id: id++, layer})
                    c += w + Math.floor(rand() * 3)
                }
            }
            for (let c = 0; c < cols; c++) fixed.put(gr + 2, c, hash(c, 3, 1, seed) < .7 ? '_' : '-', 1)
            // street lamp with a soft halo
            const lc = a0 + Math.round(aw * .18), lt = gr - 11
            for (let dr = -5; dr <= 5; dr++) for (let dc = -11; dc <= 11; dc++) {
                const d = (dc / 11) ** 2 + (dr / 5) ** 2
                if (d < 1 && d > .06 && hash(dc, dr, 4, seed) < (1 - d) * .55) fixed.put(lt + dr, lc + dc, '.', d < .4 ? 2 : 1)
            }
            for (let r = lt + 1; r <= gr; r++) fixed.put(r, lc, '|', 3)
            fixed.put(lt, lc - 1, '(', 3); fixed.put(lt, lc, 'O', 4); fixed.put(lt, lc + 1, ')', 3)
            rain = Array.from({length: Math.floor(cols * rows * .034)}, () => ({x: rand() * cols * 1.1, y: rand() * rows, v: 8 + rand() * 6}))
            glass = Array.from({length: Math.max(6, Math.floor(cols * .05))}, () => ({x: Math.floor(rand() * cols), y: rand() * vis, wait: rand() * 8, sl: 0}))
            trail = new Float32Array(rows * cols); rip = []; nextRip = 0
            clouds = new CloudsLayer(s, {n: 4, kinds: ['st'], z: [.1, .4], maxL: 2, big: 1.1, y: () => [0, Math.max(3, vis * .22)]})
            clouds.init()
        },
        draw(d, t) {
            const {cols, rows} = s
            s.begin()
            fixed.blit(); clouds?.draw(d, t)
            for (const w of wins) {
                const lit = hash(w.id, Math.floor(t / 45 + w.id * .37), 9, seed) < (w.layer ? .3 : .18)
                if (lit) { s.put(w.r, w.c, '#', w.layer ? 3 : 2); s.put(w.r, w.c + 1, '#', w.layer ? 3 : 2) }
            }
            for (const p of rain) {
                p.y += p.v * d; p.x += .2 * p.v * d
                if (p.y >= rows) { p.y = -rand() * 6; p.x = rand() * cols * 1.1 - cols * .1 }
                const x = Math.floor(p.x), y = Math.floor(p.y)
                s.putSoft(y, x, ':', 3); s.putSoft(y - 1, x - 1, '.', 2); s.putSoft(y - 2, x - 1, '.', 1)
            }
            if (t >= nextRip) { rip.push({x: Math.floor(rand() * cols), t0: t}); nextRip = t + .12 + rand() * .25 }
            for (let i = rip.length - 1; i >= 0; i--) {
                const r = rip[i], a = (t - r.t0) * 3.2
                if (a > 3.2) { rip.splice(i, 1); continue }
                const l = a < 1.5 ? 2 : 1
                s.putSoft(gr + 1, r.x - Math.round(a * 2), a < 1 ? '(' : '.', l); s.putSoft(gr + 1, r.x + Math.round(a * 2), a < 1 ? ')' : '.', l)
            }
            const dec = Math.exp(-d * .9)
            for (let i = 0; i < trail.length; i++) {
                if (trail[i] > .001) {
                    trail[i] *= dec
                    if (trail[i] < .2) trail[i] = 0; else s.putSoft(Math.floor(i / cols), i % cols, '.', trail[i] > .6 ? 2 : 1)
                }
            }
            for (const g of glass) {
                if (g.sl > 0) { const step = 7 * d; g.y += step; g.sl -= step } else { g.wait -= d; if (g.wait <= 0) { g.sl = 3 + rand() * 8; g.wait = 2 + rand() * 7 } }
                if (g.y >= vis) { g.y = rand() * vis * .4; g.x = Math.floor(rand() * cols); g.sl = 0 }
                const iy = Math.floor(g.y)
                if (iy >= 0 && iy < rows) trail[iy * cols + g.x] = 1
                s.put(iy, g.x, 'o', 3)
            }
            s.flush()
        },
    }
}
