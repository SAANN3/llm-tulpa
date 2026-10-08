import {rand} from '../engine/random.ts'
import type {Level} from '../engine/types.ts'
import type {Scene} from '../engine/scene.ts'

export interface FirefliesOptions {
    /** Fireflies per cell */
    density?: number
    drift?: boolean
    /** A glow's length, from..to seconds */
    per?: [number, number]
    /** The character for levels 1 to 4 */
    chars?: [string, string, string, string]
}

interface Firefly { per: number; ph: number; k: number; x: number; y: number; vx: number; vy: number }

/** Lights that glow up somewhere, fade and come back elsewhere */
export class FirefliesLayer {
    private readonly s: Scene
    private readonly density: number
    private readonly drift: boolean
    private readonly per: [number, number]
    private readonly chars: [string, string, string, string]
    private ff: Firefly[] = []

    constructor(s: Scene, {density = 0.028, drift = false, per = [3.5, 7.5], chars = ['•', '•', 'o', 'O']}: FirefliesOptions = {}) {
        this.s = s
        this.density = density
        this.drift = drift
        this.per = per
        this.chars = chars
    }

    init(): void {
        const {s, per} = this
        const n = Math.floor(s.cols * s.rows * this.density)
        this.ff = Array.from({length: n}, () => ({per: per[0] + rand() * (per[1] - per[0]), ph: rand(), k: -1, x: 0, y: 0, vx: 0, vy: 0}))
    }

    put(d: number, t: number): void {
        const s = this.s
        for (const f of this.ff) {
            const u = t / f.per + f.ph, k = Math.floor(u), fr = u - k
            if (k !== f.k) { f.k = k; f.x = rand() * s.cols; f.y = rand() * s.rows; f.vx = (rand() - .5) * 1.4; f.vy = (rand() - .5) * .9 }
            if (this.drift) { f.x += (f.vx + Math.sin(t * .9 + f.ph * 9) * .5) * d; f.y += f.vy * d }
            const e = Math.sin(fr * Math.PI * 2)
            if (e < .12) continue
            const l: Level = e < .4 ? 1 : e < .7 ? 2 : e < .9 ? 3 : 4
            s.put(Math.floor(f.y), Math.floor(f.x), this.chars[l - 1], l)
        }
    }
}
