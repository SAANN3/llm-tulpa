import {clamp, h3, noise, rand} from '../engine/random.ts'
import type {Effect, EffectFactory} from '../engine/types.ts'

export interface DotsOptions {
    /** Brightness of the dots, % */
    base: number
    /** Three long incommensurate waves rolling through the dots, % */
    tide: number
    /** A fixed difference per dot, the look of print, % */
    grain: number
    /** One wave bent by noise, % */
    flag: number
    /** How fast the waves travel */
    flow: number
}

/** Dot-grid paper whose dots swell and dim with slow waves (Ocean swell, Paper waves). Drawn as squares, not on the
 * character grid. */
export const dots = (p: DotsOptions): EffectFactory => (s): Effect => {
    let Td = 0
    return {
        init() { Td = rand() * 100 },
        draw(d) {
            Td += d * p.flow
            s.clear()
            const {ctx, W, H, strength: k} = s, {bg: BG, accent: AC, fg: FG} = s.colors
            const S = 12
            for (let j = 0, y0 = S / 2; y0 < H + S; j++, y0 += S) {
                for (let i = 0, x0 = S / 2; x0 < W + S; i++, x0 += S) {
                    let size = 1, bright = p.base / 100
                    if (p.flag) {
                        const warp = (noise(x0 * .0025, y0 * .0025, Td * .08) - .5) * 3.5
                        const w = Math.sin(x0 * .007 - y0 * .0025 + warp - Td * 2.0)
                        size += w * (p.flag / 100) * 1.05; bright *= 1 + w * (p.flag / 100) * .32
                    }
                    if (p.tide) {
                        const warp = (noise(x0 * .0015, y0 * .0015, Td * .03) - .5) * 3.5
                        const w1 = Math.sin(x0 * .0048 + y0 * .0019 + warp - Td * 1.35)
                        const w2 = Math.sin(x0 * .0022 - y0 * .0044 + warp * .7 + Td * 0.88)
                        const w3 = Math.sin(x0 * .0034 + y0 * .0036 - Td * 0.55)
                        const raw = w1 * .45 + w2 * .35 + w3 * .20, swell = raw < 0 ? raw * 0.35 : raw, amp = p.tide / 100
                        size += (swell * 1.25 + .1) * amp; bright *= 1 + (swell * .42 + .08) * amp
                    }
                    if (p.grain) { const g = h3(i, j, 7) - .5; size += g * (p.grain / 100) * 1.1; bright *= 1 + g * (p.grain / 100) * .5 }
                    bright *= k
                    const t = clamp(bright, 0, 1)
                    let cr = BG[0] + (AC[0] - BG[0]) * t, cg = BG[1] + (AC[1] - BG[1]) * t, cb = BG[2] + (AC[2] - BG[2]) * t
                    // the brightest dots lean toward the text color, so a crest reads as light, not just more accent
                    if (t > 0.45) { const hf = (t - 0.45) * 0.35; cr += (FG[0] - cr) * hf; cg += (FG[1] - cg) * hf; cb += (FG[2] - cb) * hf }
                    ctx.fillStyle = `rgb(${cr | 0},${cg | 0},${cb | 0})`
                    const sz = Math.max(.35, size)
                    ctx.fillRect(x0 - sz / 2, y0 - sz / 2, sz, sz)
                }
            }
        },
    }
}
