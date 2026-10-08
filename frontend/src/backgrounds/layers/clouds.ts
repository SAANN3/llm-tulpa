import {clamp, hash, noise, rand, smooth} from '../engine/random.ts'
import {level, type Scene} from '../engine/scene.ts'

export type CloudKind = 'cu' | 'st'

export interface CloudsOptions {
    /** Clouds per 160 columns */
    n?: number
    /** `cu` cumulus (a heap), `st` stratus (a long low band); picked from this list at random */
    kinds?: CloudKind[]
    /** Depth, from..to: a farther cloud is smaller, slower and dimmer */
    z?: [number, number]
    /** The brightest level a cloud reaches */
    maxL?: number
    /** Rain falling from the clouds, 0 for none */
    rain?: number
    /** Size factor */
    big?: number
    speed?: number
    /** The rows a cloud's top may start in, for the scene's current size; 2 to half the height without it */
    y?: (s: Scene) => [number, number]
}

/** One puff of a cloud's outline: centre x, centre y, radius x, radius y */
type Puff = [number, number, number, number]

interface Cloud { zz: number; Wc: number; Hc: number; pf: Puff[]; ns: number; seed: number; y: number; v: number; x: number }
interface Drop { x: number; y: number; v: number }

const RAMP = ['.', ':', '-', '=', '+', '*', 'o', 'O']

/** Volumetric clouds: filled, lighter on top and darker below, edges that drift and slowly change shape */
export class CloudsLayer {
    private readonly s: Scene
    private readonly n: number
    private readonly kinds: CloudKind[]
    private readonly z: [number, number]
    private readonly maxL: number
    private readonly rain: number
    private readonly big: number
    private readonly speed: number
    private readonly y: (s: Scene) => [number, number]
    private cl: Cloud[] = []
    private drops: Drop[] = []
    private racc = 0

    constructor(s: Scene, {n = 5, kinds = ['cu', 'cu', 'st'], z = [.2, 1], maxL = 4, rain = 0, big = 1, speed = 1, y}: CloudsOptions = {}) {
        this.s = s
        this.n = n
        this.kinds = kinds
        this.z = z
        this.maxL = maxL
        this.rain = rain
        this.big = big
        this.speed = speed
        this.y = y ?? ((sc) => [2, sc.rows * .5])
    }

    private make(fresh: boolean): Cloud {
        const {z, kinds, big} = this
        const zz = z[0] + rand() * (z[1] - z[0]), kind = kinds[Math.floor(rand() * kinds.length)], sc = (.55 + .85 * zz) * big
        let Hc: number, Wc: number, np: number
        if (kind === 'st') { Hc = Math.max(8, Math.round(11 * sc)); Wc = Math.round((40 + rand() * 25) * sc); np = 6 + Math.floor(rand() * 3) }
        else { Hc = Math.max(9, Math.round((14 + rand() * 4) * sc)); Wc = Math.round(Hc * (2.6 + rand() * .7)); np = 4 + Math.floor(rand() * 2) }
        const pf: Puff[] = []
        for (let i = 0; i < np; i++) {
            const u = (i + .5) / np, mid = 1 - Math.abs(u - .5) * 2, px = Wc * (.14 + .72 * u) + (rand() - .5) * Wc * .05
            let ry: number, rx: number, py: number
            if (kind === 'st') { ry = Hc * (.24 + .14 * mid) * (.94 + rand() * .12); rx = Wc / np * (1.6 + rand() * .3); py = Hc * .74 - ry * .6 }
            else { ry = Hc * (.2 + .26 * mid * mid) * (.93 + rand() * .14); rx = ry * (2.0 + rand() * .2); py = Hc * .8 - ry * .75 }
            pf.push([px, py, rx, ry])
        }
        const [y0, y1] = this.y(this.s)
        return {
            zz, Wc, Hc, pf, ns: rand() * 200, seed: Math.floor(rand() * 1e6), y: Math.round(y0 + rand() * Math.max(0, y1 - y0 - Hc * .5)),
            v: (.25 + .75 * zz) * this.speed, x: fresh ? rand() * (this.s.cols + Wc) - Wc : -Wc - 2,
        }
    }

    /** How much of the cloud is at (x, y): the puffs' union, faded out toward the flat bottom */
    private field(c: Cloud, x: number, y: number): number {
        let q = 1
        for (const p of c.pf) { const dx = (x - p[0]) / p[2], dy = (y - p[1]) / p[3], d = dx * dx + dy * dy; if (d < 1) q *= d }
        return (1 - q) * smooth(c.Hc * .88, c.Hc * .7, y)
    }

    init(): void {
        this.cl = Array.from({length: Math.max(2, Math.round(this.n * this.s.cols / 160))}, () => this.make(true)).sort((a, b) => a.zz - b.zz)
        this.drops = []
        this.racc = 0
    }

    draw(d: number, t: number): void {
        const s = this.s
        const {cols, rows} = s
        const tm = t * .035
        for (let k = 0; k < this.cl.length; k++) {
            const c = this.cl[k]
            c.x += c.v * d
            if (c.x > cols + 2) { this.cl[k] = this.make(false); continue }
            const ox = Math.floor(c.x)
            for (let ly = 0; ly < c.Hc; ly++) {
                const r = c.y + ly
                if (r < 0 || r >= rows) continue
                for (let lx = 0; lx < c.Wc; lx++) {
                    const col = ox + lx
                    if (col < 0 || col >= cols) continue
                    const X = lx + (noise(lx * .09 + c.ns, ly * .16, tm) - .5) * 2.6, Y = ly + (noise(lx * .09 + c.ns + 50, ly * .16 + 9, tm) - .5) * 1.8
                    const f0 = this.field(c, X, Y)
                    if (f0 < .14 + .06 * hash(lx, ly, 3, c.seed)) continue
                    const fl = this.field(c, X - 3.6, Y - 1.8)
                    let b = .3 + .34 * Math.pow(1 - clamp(Y / c.Hc, 0, 1), 1.1) + .7 * (f0 - fl) + .1 * f0 + (hash(lx, ly, 8, c.seed) - .5) * .06
                    if (f0 < .32) b = Math.min(b, .26)
                    b = clamp(b - .08, 0, .999)
                    s.put(r, col, RAMP[Math.floor(b * 8)], level(clamp(Math.round(.7 + b * this.maxL * (.55 + .45 * c.zz)), 1, this.maxL)))
                }
            }
        }
        if (this.rain) {
            this.racc += d * cols * this.rain * 2
            while (this.racc >= 1) {
                this.racc -= 1
                const c = this.cl[Math.floor(rand() * this.cl.length)], x = c.x + c.Wc * (.1 + .8 * rand())
                if (x >= 0 && x < cols && this.drops.length < 900) this.drops.push({x, y: c.y + c.Hc * .85, v: 7 + rand() * 6})
            }
            for (let i = this.drops.length - 1; i >= 0; i--) {
                const p = this.drops[i]
                p.y += p.v * d; p.x += .8 * d
                if (p.y >= rows) { this.drops.splice(i, 1); continue }
                s.putIfEmpty(Math.floor(p.y), Math.floor(p.x), ':', 3)
                s.putIfEmpty(Math.floor(p.y) - 1, Math.floor(p.x - .8), '.', 2)
            }
        }
    }
}
