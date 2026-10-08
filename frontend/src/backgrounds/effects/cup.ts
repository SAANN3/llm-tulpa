import {clamp, rand} from '../engine/random.ts'
import type {Effect, EffectFactory} from '../engine/types.ts'

const RAMP = ['.', ':', '-', '=', '+', '*', '#']

interface Puff { x: number; y: number; a: number; ph: number; sw: number }
interface Place { cx: number; cy: number; k: number }

/** A big "3D" cup of tea with steam: a shaded cylinder, a saucer, a handle, a slow swirl on the surface. One in the
 * middle of a chat page, one on each side of a centered page */
export const cup = (): EffectFactory => (s): Effect => {
    let puffs: Puff[] = [], next = 0

    const centers = (): Place[] => {
        const vis = s.visibleRows(), cy = Math.round(vis * .5) + 1
        if (s.layout.kind === 'chat') return [{cx: s.chatCenterCol(), cy, k: 1}]
        const side = s.sideCols()
        return [{cx: Math.round(side / 2), cy, k: .62}, {cx: s.cols - Math.round(side / 2) - 1, cy, k: .62}]
    }

    function drawCup(cx: number, cy: number, k: number, t: number) {
        const a = Math.round(14 * k), b = Math.max(2, Math.round(3 * k)), H = Math.round(9 * k), ab = Math.round(11 * k)
        const as = Math.round(24 * k), bs = Math.max(2, Math.round(3 * k)), sy = cy + H + 1
        // saucer
        for (let dr = -bs; dr <= bs; dr++) for (let dc = -as; dc <= as; dc++) {
            const dd = (dc / as) ** 2 + (dr / bs) ** 2
            if (dd > 1) continue
            if (dd > .78) s.put(sy + dr, cx + dc, Math.abs(dc) > as * .86 ? (dc < 0 ? '(' : ')') : dr < 0 ? '-' : '_', 3); else s.put(sy + dr, cx + dc, '.', 1)
        }
        // body, lit from the left
        const hwAt = (r: number) => Math.round(a - (a - ab) * Math.pow(r / H, 1.1))
        for (let r = 1; r <= H; r++) {
            const hw = hwAt(r)
            for (let c = -hw; c <= hw; c++) {
                if (c === -hw) { s.put(cy + r, cx + c, '\\', 4); continue }
                if (c === hw) { s.put(cy + r, cx + c, '/', 4); continue }
                const u = c / hw, sh = clamp(.95 - Math.abs(u + .35) * .9, 0, 1)
                s.put(cy + r, cx + c, RAMP[Math.floor(sh * 6.99)], sh > .6 ? 3 : 2)
            }
        }
        s.put(cy + H + 1, cx - ab + 1, '\\', 4); s.put(cy + H + 1, cx + ab - 1, '/', 4)
        for (let c = -ab + 2; c <= ab - 2; c++) s.put(cy + H + 1, cx + c, '_', 4)
        // handle: a clear "C" on the right
        {
            const hr = cy + Math.round(H * .45), hx = cx + hwAt(Math.round(H * .45)) + 1, rx = Math.max(4, Math.round(6 * k)), ry = Math.max(2, Math.round(3.5 * k))
            for (let dr = -ry; dr <= ry; dr++) for (let dc = 0; dc <= rx; dc++) {
                const dd = (dc / rx) ** 2 + (dr / ry) ** 2
                if (dd > 1 || dd < .4) continue
                s.put(hr + dr, hx + dc, dd > .72 ? (Math.abs(dr) === ry || dc < 2 ? (dr < 0 ? '_' : '-') : ')') : (dr < 0 ? '-' : '_'), 4)
            }
        }
        // the rim and the tea's surface with a slow swirl
        for (let dr = -b; dr <= b; dr++) for (let dc = -a; dc <= a; dc++) {
            const dd = (dc / a) ** 2 + (dr / b) ** 2
            if (dd > 1) continue
            if (dd > .8) s.put(cy + dr, cx + dc, Math.abs(dc) > a * .88 ? (dc < 0 ? '(' : ')') : dr < 0 ? '_' : '-', 4)
            else {
                const ang = Math.atan2(dr * 2.05, dc), w = Math.sin(ang * 2 + Math.sqrt(dd) * 7 - t * .6)
                s.put(cy + dr, cx + dc, w > .6 ? '~' : '.', w > .6 ? 3 : 2)
            }
        }
    }

    return {
        init() { puffs = []; next = 0 },
        draw(d, t) {
            s.begin()
            const cs = centers()
            cs.forEach(c => drawCup(c.cx, c.cy, c.k, t))
            if (t >= next) {
                const c = cs[Math.floor(rand() * cs.length)]
                puffs.push({x: c.cx + (rand() - .5) * 14 * c.k * .9, y: c.cy - 1, a: 0, ph: rand() * 6, sw: .6 + rand() * .8})
                next = t + .2 / cs.length
            }
            for (let i = puffs.length - 1; i >= 0; i--) {
                const p = puffs[i]
                p.a += d / 8; p.y -= 1.9 * d; p.x += (Math.sin(t * .8 + p.ph) * p.sw * 1.3 + .15) * d
                if (p.a > 1) { puffs.splice(i, 1); continue }
                const A = p.a, ch = A < .15 ? 'o' : A < .5 ? 'O' : A < .75 ? 'o' : A < .9 ? '.' : '·', l = A < .15 ? 2 : A < .5 ? 3 : A < .75 ? 2 : 1, x = Math.floor(p.x), y = Math.floor(p.y)
                s.put(y, x, ch, l)
                if (A > .12 && A < .6) s.put(y, x + 1, ch, l)
            }
            s.flush()
        },
    }
}
