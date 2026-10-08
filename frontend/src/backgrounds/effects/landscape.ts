import {hash, noise, rand} from '../engine/random.ts'
import type {Effect, EffectFactory, Level} from '../engine/types.ts'
import {CloudsLayer} from '../layers/clouds.ts'
import {StarsLayer} from '../layers/stars.ts'

export interface LandscapeOptions {
    /** A moon and stars, or the sun coming up */
    night?: boolean
    /** Snow on the trees, a frozen lake, falling snow and northern lights */
    winter?: boolean
}

interface Ridge { base: number; amp: number; f: number }
interface Smoke { x: number; y: number; a: number; ph: number }
interface Glow { per: number; ph: number; k: number; x: number; y: number }
interface Bird { x: number; y: number; ph: number }
interface Flake { x: number; y: number; v: number; ph: number; s: string }

const CAB = ['      __    ', '  /\\  |  |  ', ' /  \\_|__|  ', '/______\\    ', '| [##] |    ', '|______|    ']
const BOAT = ['    |\\   ', ' ___|_\\__', ' \\______/']

/** A whole cozy scene filling the window: three ridges with mist between them, pines, a cabin with a lit window and
 * smoke, a lake with the moon's or the sun's path on it, a boat, and stars, fireflies or birds */
export const landscape = ({night = true, winter = false}: LandscapeOptions = {}): EffectFactory => (s): Effect => {
    let snow: Flake[] = []
    let sg: string[] = [], sv: Uint8Array = new Uint8Array(0), seed = 0, vis = 0, lt = 0, a0 = 0, aw = 0, mc = 0, mr = 0
    let cab: { c0: number; r0: number } = {c0: 0, r0: 0}, sky: StarsLayer | null = null, clouds: CloudsLayer | null = null
    let ff: Glow[] = [], smoke: Smoke[] = [], nextPuff = 0, birds: Bird[] = [], hillY: number[] = []
    const sput = (r: number, c: number, ch: string, l: Level) => {
        if (!s.inside(r, c)) return
        const i = r * s.cols + c
        sg[i] = ch; sv[i] = l
    }
    const ridge = (c: number, o: Ridge, k: number) => (o.base + o.amp * ((noise(c * o.f + seed, k * 7.3, 1) - .5) * 1.7 + (noise(c * o.f * 2.9 + seed, k * 3.1, 2) - .5) * .7)) * vis
    function pine(cx: number, base: number, h: number) {
        for (let i = 0; i < h; i++) {
            const r = base - h + 1 + i
            if (i === h - 1) { sput(r, cx, '|', 3); continue }
            if (i === 0) { sput(r, cx, '^', 3); continue }
            const half = Math.ceil(i * .75)
            for (let k = -half; k <= half; k++) sput(r, cx + k, k === -half ? '/' : k === half ? '\\' : (hash(k, i, cx, seed | 0) < .5 ? (winter ? '*' : '%') : (winter ? '+' : '#')), k === -half || k === half ? 3 : 2)
        }
    }
    return {
        init() {
            const {cols, rows} = s
            vis = s.visibleRows(); a0 = s.areaX(); aw = cols - a0; seed = rand() * 500
            sg = new Array<string>(cols * rows).fill(' '); sv = new Uint8Array(cols * rows)
            const R: Ridge[] = [{base: .34, amp: .15, f: .03}, {base: .46, amp: .1, f: .045}, {base: .6, amp: .05, f: .06}]
            lt = Math.round(.76 * vis)
            const ridges = R.map((o, k) => Array.from({length: cols}, (_, c) => ridge(c, o, k)))
            hillY = ridges[2]
            const dens = [1, 1, 1], fillCh = winter ? ['.', '.', ':'] : ['.', ':', '%']
            ridges.forEach((rg, k) => {
                for (let c = 0; c < cols; c++) {
                    const r0 = Math.round(rg[c]), sl = Math.round(rg[Math.min(cols - 1, c + 1)]) - Math.round(rg[Math.max(0, c - 1)])
                    for (let r = r0; r < rows; r++) {
                        if (r === r0) {
                            const peak = rg[c] < rg[Math.max(0, c - 1)] && rg[c] <= rg[Math.min(cols - 1, c + 1)]
                            sput(r, c, peak ? '^' : sl < 0 ? '/' : sl > 0 ? '\\' : '_', k === 0 ? 2 : 3)
                        } else sput(r, c, hash(c, r, k, seed | 0) < dens[k] ? fillCh[k] : ' ', 1)
                    }
                }
            })
            for (let r = lt; r < rows; r++) for (let c = 0; c < cols; c++) sput(r, c, ' ', 1)   // lake: erased here, the water is drawn live
            for (let c = 0; c < cols; c++) if (hash(c, 1, 2, seed | 0) < .8) sput(lt - 1, c, '_', 2)
            // celestial body
            const body = night ? {rx: 9, ry: 4, cr: Math.round(.17 * vis), cc: a0 + Math.round(.72 * aw)} : {rx: 7.5, ry: 3.2, cr: Math.round(.31 * vis), cc: a0 + Math.round(.62 * aw)}
            mr = body.cr; mc = body.cc
            for (let dr = -Math.ceil(body.ry * 2.6); dr <= Math.ceil(body.ry * 2.6); dr++) for (let dc = -Math.ceil(body.rx * 2.6); dc <= Math.ceil(body.rx * 2.6); dc++) {
                const d = (dc / body.rx) ** 2 + (dr / body.ry) ** 2, hh = hash(dc, dr, 5, seed | 0)
                if (d <= 1) sput(mr + dr, mc + dc, night ? (hh < .2 ? 'o' : hh < .6 ? ':' : '.') : (hh < .5 ? '#' : '%'), d > .7 ? 2 : 3)
                else if (d < 2.6 && hh < .1 * (2.6 - d)) sput(mr + dr, mc + dc, '.', 1)
            }
            // trees and the cabin
            const cabC = a0 + Math.round(.3 * aw)
            cab = {c0: cabC - 6, r0: lt - 6}
            CAB.forEach((row, i) => { for (let k = 0; k < row.length; k++) if (row[k] !== ' ') sput(cab.r0 + i, cab.c0 + k, row[k], 4) })
            const np = Math.max(6, Math.round(aw / 11))
            for (let i = 0; i < np; i++) {
                const cx = a0 + 2 + Math.floor(hash(i, 3, 7, seed | 0) * (aw - 4))
                if (Math.abs(cx - cabC) < 11) continue
                pine(cx, lt - 1 - Math.floor(hash(i, 4, 7, seed | 0) * 2), 5 + Math.floor(hash(i, 5, 7, seed | 0) * 5))
            }
            for (let i = 0; i < Math.round(aw / 12); i++) {
                const cx = a0 + 1 + Math.floor(hash(i, 8, 3, seed | 0) * (aw - 2))
                if (Math.abs(cx - cabC) < 8) continue
                const base = Math.min(lt - 3, Math.round(hillY[cx]) + 4 + Math.floor(hash(i, 9, 3, seed | 0) * 3))
                pine(cx, base, 3 + Math.floor(hash(i, 6, 3, seed | 0) * 2))
            }
            sky = night ? new StarsLayer(s, {gap: [9, 18], density: .03}) : null
            sky?.init()
            const ceil = (sc: typeof s): [number, number] => [1, Math.max(3, sc.visibleRows() * .3)]
            clouds = night
                ? new CloudsLayer(s, {n: 3, kinds: ['st', 'cu'], z: [.15, .55], maxL: 2, big: .55, y: ceil})
                : new CloudsLayer(s, {n: 5, kinds: ['cu', 'cu', 'st'], z: [.2, .7], maxL: 3, big: .62, y: ceil})
            clouds.init()
            ff = night && !winter ? Array.from({length: Math.round(aw * .24)}, () => ({per: 3.5 + rand() * 4, ph: rand(), k: -1, x: 0, y: 0})) : []
            birds = night ? [] : Array.from({length: 5}, (_, i) => ({x: a0 + aw * .1 - i * 4, y: Math.round(.2 * vis) + (i % 2 ? 1 : -1) * Math.ceil(i / 2) * 1.5, ph: rand() * 3}))
            smoke = []; nextPuff = 0
            snow = winter ? Array.from({length: Math.floor(cols * rows * .012)}, () => ({x: rand() * cols, y: rand() * rows, v: .8 + rand() * 1.4, ph: rand() * 6, s: ['*', '+', '.'][Math.floor(rand() * 3)]})) : []
        },
        draw(d, t) {
            const {cols, rows} = s
            s.begin()
            sky?.draw(d, t)
            for (let i = 0; i < sv.length; i++) if (sv[i]) { s.chars[i] = sg[i]; s.levels[i] = sv[i] }
            if (!winter) clouds?.draw(d, t)
            // northern lights
            if (winter) for (let c = a0; c < cols; c++) {
                const n = noise(c * .06 + t * .05, 3.3, 4)
                if (n < .42) continue
                const h = Math.round(3 + (n - .42) * 22), top = Math.round(vis * .08 + Math.sin(c * .045 + t * .09) * 3 + noise(c * .02, t * .03, 6) * 5)
                for (let k = 0; k < h; k++) {
                    const r = top + k
                    if (r >= vis * .5) break
                    if (s.levels[r * cols + c] > 2) continue
                    s.put(r, c, k < 2 ? ':' : k < h * .7 ? '.' : ',', k < h * .4 ? 2 : 1)
                }
            }
            // mist bands between the ridges
            for (const band of [Math.round(.5 * vis), Math.round(.63 * vis)]) for (let rr = -1; rr <= 1; rr++) for (let c = 0; c < cols; c++) {
                const r = band + rr, i = r * cols + c
                if (r >= 0 && r < rows && s.levels[i] <= 2 && noise(c * .05 - t * .06, rr * .9 + band * .3, 9) > .64) { s.chars[i] = rr === 0 ? '~' : '-'; s.levels[i] = 1 }
            }
            // water: slow shimmer + the reflection of the moon/sun
            if (winter) for (let r = lt; r < rows; r++) for (let c = 0; c < cols; c++) {
                const h0 = hash(c, r, 11, seed | 0)
                if (h0 < .07) s.put(r, c, '-', 1); else if (h0 < .085 && Math.sin(t * .8 + h0 * 400) > .6) s.put(r, c, '*', 3)
            }
            for (let r = lt; r < (winter ? lt : rows); r++) {
                const u = (r - lt) / Math.max(1, rows - lt), thr = Math.max(.95, 1.5 - u * .6)
                for (let c = 0; c < cols; c++) {
                    const w = Math.sin(c * .31 + r * 1.1 - t * .8) + Math.sin(c * .11 - t * .45 + r * .35)
                    if (w > thr) s.put(r, c, w > thr + .35 ? '-' : '~', w > thr + .35 ? 2 : 1)
                }
                const hw = 2 + u * 14
                for (let c = Math.floor(mc - hw); c <= mc + hw; c++) {
                    const sh = Math.sin(c * 1.3 + r * 2.1 - t * 1.6) + (hash(c, r, 4, seed | 0) - .5) * .8
                    if (sh > -.25 + u * .9 && Math.abs(c - mc) < hw * (.55 + .45 * hash(r, 2, 6, seed | 0))) s.put(r, c, sh > .5 ? '=' : '-', sh > .5 ? 3 : 2)
                }
            }
            // boat
            if (!winter) {
                const bx = a0 + Math.round(.55 * aw + Math.sin(t * .05) * 8), by = lt + 3 + Math.round(Math.sin(t * .5) * .6)
                s.art(BOAT, by, bx, 3)
            }
            // cabin window glow + chimney smoke
            {
                const glow = .5 + .5 * Math.sin(t * 1.3) * Math.sin(t * .37 + 1)
                for (let k = 0; k < 2; k++) s.put(cab.r0 + 4, cab.c0 + 4 + k, '#', glow > .55 ? 4 : 3)
                s.put(cab.r0 + 4, cab.c0 + 3, '[', 4); s.put(cab.r0 + 4, cab.c0 + 6, ']', 4)
            }
            if (t >= nextPuff) { smoke.push({x: cab.c0 + 7.5, y: cab.r0 - 1, a: 0, ph: rand() * 6}); nextPuff = t + 1.1 + rand() * .8 }
            for (let i = smoke.length - 1; i >= 0; i--) {
                const p = smoke[i]
                p.a += d / 8; p.y -= 1.25 * d; p.x += (.6 + Math.sin(t * .6 + p.ph) * .5) * d
                if (p.a > 1) { smoke.splice(i, 1); continue }
                s.put(Math.floor(p.y), Math.floor(p.x), p.a < .3 ? 'O' : p.a < .6 ? 'o' : p.a < .85 ? '.' : '·', p.a < .6 ? 3 : p.a < .85 ? 2 : 1)
            }
            // fireflies on the shore / birds in the morning
            for (const f of ff) {
                const u = t / f.per + f.ph, k = Math.floor(u), fr = u - k
                if (k !== f.k) { f.k = k; f.x = a0 + rand() * aw; f.y = lt - 9 + rand() * 8 }
                const e = Math.sin(fr * Math.PI * 2)
                if (e < .12) continue
                s.put(Math.floor(f.y), Math.floor(f.x), e < .4 ? '·' : e < .7 ? 'o' : '•', e < .4 ? 1 : e < .7 ? 2 : 3)
            }
            for (const b of birds) {
                b.x += 1.5 * d
                if (b.x > cols + 6) b.x = a0 - 8
                s.put(Math.round(b.y + Math.sin(t * .4 + b.ph) * .8), Math.floor(b.x), Math.floor(t * 2.4 + b.ph * 3) % 2 ? 'v' : '^', 3)
            }
            for (const f of snow) {
                f.y += f.v * d; f.x += Math.sin(t * .7 + f.ph) * .6 * d
                if (f.y >= rows) { f.y = -1; f.x = rand() * cols }
                s.putIfEmpty(Math.floor(f.y), Math.floor(f.x), f.s, f.s === '.' ? 1 : 2)
            }
            s.flush()
        },
    }
}
