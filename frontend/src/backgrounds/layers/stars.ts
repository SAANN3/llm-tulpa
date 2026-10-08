import {rand} from '../engine/random.ts'
import type {Scene} from '../engine/scene.ts'

export interface StarsOptions {
    /** Seconds between shooting stars, from..to */
    gap?: [number, number]
    /** Up to this many shooting stars at once */
    multi?: number
    /** Stars per cell */
    density?: number
}

interface Star { r: number; c: number; per: number; ph: number; big: 0 | 1 | 2 }
interface Shooter { x: number; y: number; vx: number; vy: number }

/** Twinkling stars and, now and then, a shooting star with a fading trail */
export class StarsLayer {
    private readonly s: Scene
    private readonly gap: [number, number]
    private readonly multi: number
    private readonly density: number
    private st: Star[] = []
    private trail: Float32Array = new Float32Array(0)
    private shooters: Shooter[] = []
    private next = 4

    constructor(s: Scene, {gap = [5, 12], multi = 1, density = .035}: StarsOptions = {}) {
        this.s = s
        this.gap = gap
        this.multi = multi
        this.density = density
    }

    private spawn(): void {
        const s = this.s
        this.shooters.push({x: s.cols * (.3 + rand() * .75), y: rand() * s.rows * .35, vx: -(16 + rand() * 9), vy: 6 + rand() * 5})
    }

    init(): void {
        const s = this.s
        this.st = Array.from({length: Math.floor(s.cols * s.rows * this.density)}, () => {
            const z = rand()
            return {r: Math.floor(rand() * s.rows), c: Math.floor(rand() * s.cols), per: 4 + rand() * 7, ph: rand(), big: z > .92 ? 2 : z > .68 ? 1 : 0}
        })
        this.trail = new Float32Array(s.rows * s.cols)
        this.shooters = []
        this.next = 2 + rand() * 4
    }

    draw(d: number, t: number): void {
        const {s, trail} = this
        const {cols, rows} = s
        for (const st of this.st) {
            const e = .5 + .5 * Math.sin((t / st.per + st.ph) * Math.PI * 2)
            if (st.big === 0) { if (e > .55) s.put(st.r, st.c, '·', 2) }
            else if (st.big === 1) s.put(st.r, st.c, e > .65 ? '+' : '·', e > .65 ? 2 : 1)
            else s.put(st.r, st.c, e > .75 ? '*' : e > .45 ? '+' : '·', e > .75 ? 3 : e > .45 ? 2 : 1)
        }
        if (t >= this.next) {
            const n = 1 + Math.floor(rand() * this.multi)
            for (let i = 0; i < n; i++) this.spawn()
            this.next = t + this.gap[0] + rand() * (this.gap[1] - this.gap[0])
        }
        for (let k = this.shooters.length - 1; k >= 0; k--) {
            const sh = this.shooters[k], ox = sh.x, oy = sh.y
            sh.x += sh.vx * d; sh.y += sh.vy * d
            const n = Math.ceil(Math.max(Math.abs(sh.x - ox), Math.abs(sh.y - oy))) + 1
            for (let i = 0; i <= n; i++) {
                const x = Math.floor(ox + (sh.x - ox) * i / n), y = Math.floor(oy + (sh.y - oy) * i / n)
                if (x >= 0 && x < cols && y >= 0 && y < rows) trail[y * cols + x] = 1
            }
            if (sh.x < -2 || sh.y > rows * .85) this.shooters.splice(k, 1)
        }
        const dec = Math.exp(-d * 2.4)
        for (let i = 0; i < trail.length; i++) {
            if (trail[i] <= 0.001) continue
            trail[i] *= dec
            const v = trail[i]
            if (v < .12) { trail[i] = 0; continue }
            s.chars[i] = v > .9 ? '*' : v > .55 ? '+' : v > .3 ? '.' : '·'
            s.levels[i] = v > .9 ? 4 : v > .55 ? 3 : v > .3 ? 2 : 1
        }
    }
}
