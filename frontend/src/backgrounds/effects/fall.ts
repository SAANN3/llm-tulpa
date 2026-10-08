import {rand} from '../engine/random.ts'
import type {Effect, EffectFactory, Level} from '../engine/types.ts'

export interface FallOptions {
    /** Speed in rows per second, from..to */
    v: [number, number]
    /** Sideways drift per unit of speed (negative: to the left) */
    wind: number
    /** Side-to-side wobble, 0 for none */
    wob: number
    /** Flakes or drops per column */
    density: number
    /** How fast the trail fades */
    decay: number
    /** Levels of the head, the middle and the tail of the trail */
    lv: [Level, Level, Level]
    /** Characters of the head, the middle and the tail */
    chars: [string, string, string]
}

interface Flake { x: number; y: number; v: number; ph: number; big: boolean }

/** Soft rain or snow: each drop leaves a short trail that fades */
export const fall = (p: FallOptions): EffectFactory => (s): Effect => {
    let trail: Float32Array = new Float32Array(0), drops: Flake[] = []
    const make = (fresh: boolean): Flake => ({x: rand() * s.cols * (p.wind < 0 ? 1.3 : 1), y: fresh ? rand() * s.rows : -rand() * s.rows * .6, v: p.v[0] + rand() * (p.v[1] - p.v[0]), ph: rand() * 6, big: rand() < .35})
    return {
        init() { trail = new Float32Array(s.rows * s.cols); drops = Array.from({length: Math.floor(s.cols * p.density)}, () => make(true)) },
        draw(d, t) {
            s.begin()
            const {cols, rows} = s
            const dec = Math.exp(-d * p.decay)
            for (let i = 0; i < trail.length; i++) trail[i] *= dec
            for (const dr of drops) {
                dr.y += dr.v * d; dr.x += p.wind * dr.v * d + (p.wob ? Math.sin(t * .7 + dr.ph) * p.wob * d : 0)
                if (dr.y >= rows + 1 || dr.x < -2 || dr.x > cols + 2) { Object.assign(dr, make(false), {y: -rand() * 4}); continue }
                const r = Math.floor(dr.y), c = Math.floor(dr.x)
                if (r >= 0 && r < rows && c >= 0 && c < cols) trail[r * cols + c] = dr.big ? 1 : .6
            }
            for (let i = 0; i < trail.length; i++) {
                const v = trail[i]
                if (v < .12) continue
                if (v > .75) { s.chars[i] = p.chars[0]; s.levels[i] = p.lv[0] } else if (v > .4) { s.chars[i] = p.chars[1]; s.levels[i] = p.lv[1] } else { s.chars[i] = p.chars[2]; s.levels[i] = p.lv[2] }
            }
            s.flush()
        },
    }
}
