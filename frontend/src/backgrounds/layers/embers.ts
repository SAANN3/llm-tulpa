import {rand} from '../engine/random.ts'
import type {Scene} from '../engine/scene.ts'

export interface EmbersOptions {
    /** How far an ember rises before it is out, as a share of the window's height, from..to */
    travel?: [number, number]
    /** Speed in rows per second, from..to */
    v?: [number, number]
    /** How many, for the scene's current size; 0.62 per column without it */
    count?: (s: Scene) => number
    /** Where embers start: a point and a spread; the whole bottom edge without it */
    origin?: (() => { x: number; y: number; w: number }) | null
}

interface Ember { x: number; y0: number; travel: number; v: number; ph: number; a: number; wind: number }

/** Sparks rising and dying out */
export class EmbersLayer {
    private readonly s: Scene
    private readonly travel: [number, number]
    private readonly v: [number, number]
    private readonly count: (s: Scene) => number
    private readonly origin: (() => { x: number; y: number; w: number }) | null
    private ps: Ember[] = []

    constructor(s: Scene, {travel = [.35, .85], v = [1.4, 3.6], count, origin = null}: EmbersOptions = {}) {
        this.s = s
        this.travel = travel
        this.v = v
        this.count = count ?? ((sc) => Math.floor(sc.cols * .62))
        this.origin = origin
    }

    private make(fresh: boolean): Ember {
        const s = this.s
        const o = this.origin ? this.origin() : {x: rand() * s.cols, y: s.rows - 1, w: 0}
        return {
            x: o.x + (rand() - .5) * o.w, y0: o.y, travel: s.rows * (this.travel[0] + rand() * (this.travel[1] - this.travel[0])),
            v: this.v[0] + rand() * (this.v[1] - this.v[0]), ph: rand() * 6, a: fresh ? rand() : 0, wind: (rand() - .3) * 5,
        }
    }

    init(): void {
        this.ps = Array.from({length: this.count(this.s)}, () => this.make(true))
    }

    put(d: number): void {
        const s = this.s
        for (let i = 0; i < this.ps.length; i++) {
            const p = this.ps[i]
            p.a += p.v / p.travel * d
            if (p.a >= 1) { this.ps[i] = this.make(false); continue }
            const y = p.y0 - p.a * p.travel, x = p.x + Math.sin(p.a * 9 + p.ph) * 1.6 + p.a * p.wind, a = p.a
            s.put(Math.floor(y), Math.floor(x), a < .15 ? '*' : a < .45 ? '+' : a < .75 ? '.' : '·', a < .15 ? 4 : a < .45 ? 3 : a < .75 ? 2 : 1)
        }
    }
}
