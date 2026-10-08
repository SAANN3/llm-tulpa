import {hash, rand} from '../engine/random.ts'
import type {Scene} from '../engine/scene.ts'
import type {Effect, EffectFactory} from '../engine/types.ts'
import {EmbersLayer} from '../layers/embers.ts'
import {FirefliesLayer} from '../layers/fireflies.ts'
import {StarsLayer} from '../layers/stars.ts'

/** A little ASCII thing standing on the floor above the composer */
interface Prop { init(): void; draw(d: number, t: number): void }
type PropFactory = (s: Scene) => Prop

interface Smoke { x: number; y: number; a: number; ph: number }

const sleepingCat: PropFactory = (s) => {
    const CAT = [' /\\_/\\ ', '( -.- )', '(")_(")']
    return {
        init() {},
        draw(_d, t) {
            const c0 = s.cols - 18, r0 = s.floorRow() - 2
            s.art(CAT, r0, c0, 5)
            s.put(r0 + 2, c0 + 7, Math.floor(t * .5) % 2 ? ',' : '~', 5)
            for (let k = 0; k < 3; k++) {
                const u = ((t / 7 + k / 3) % 1 + 1) % 1
                s.put(Math.round(r0 - 1 - u * 6), Math.round(c0 + 7 + u * 4 + Math.sin(u * 6) * .8), u < .55 ? 'z' : 'Z', u < .12 || u > .85 ? 1 : 3)
            }
        },
    }
}

const owl: PropFactory = (s) => ({
    init() {},
    draw(_d, t) {
        const by = 10, bx = s.cols - 34, oc = bx + 11
        for (let c = bx; c < s.cols; c++) s.put(by, c, c % 9 === 4 ? '/' : '=', 5)
        for (let c = bx + 3; c < s.cols; c += 6) s.put(by + 1, c, '\\', 1)
        // leaves at the end of the branch, soft edges
        const lc = s.cols - 7, lr = by - 1
        for (let dr = -3; dr <= 3; dr++) for (let dc = -8; dc <= 8; dc++) {
            const dd = (dr / 3) ** 2 + (dc / 8) ** 2, hh = hash(dr, dc, 3, 77)
            if (dd > 1 + (hh - .5) * .4) continue
            s.putIfEmpty(lr + dr, lc + dc, ['&', '@', '%', '*', '~', 'o'][Math.floor(hh * 6)], dd > .75 ? 1 : 2)
        }
        const blink = Math.floor(t * 2) % 13 === 0
        s.art([' ,_, ', blink ? '(-,-)' : '(o,o)', '(   )', ' " " '], by - 4, oc, 5)
    },
})

const campfire: PropFactory = (s) => {
    const FR = [['   )   ', '  ) (  ', ' ( * ) ', '(*#*#*)'], ['   (   ', '  ( )  ', ' ) * ( ', '(#*#*#)'], ['   ^   ', '  ) (  ', ' ( * ) ', '(*#*#*)']]
    const pos = () => ({c: Math.round(s.cols * (s.layout.kind === 'chat' ? .62 : .5)), base: s.floorRow()})
    const sparks = new EmbersLayer(s, {travel: [.18, .55], v: [1.6, 3.4], count: () => 34, origin: () => { const q = pos(); return {x: q.c, y: q.base - 6, w: 4} }})
    return {
        init() { sparks.init() },
        draw(d, t) {
            const {c, base} = pos()
            s.art(['/=\\/=\\'], base, c - 3, 5)
            const fr = FR[Math.floor(t * 2.2) % 3]
            fr.forEach((row, i) => { for (let k = 0; k < row.length; k++) if (row[k] !== ' ') s.put(base - 4 + i, c - 3 + k, row[k], i < 2 ? 3 : 4) })
            sparks.put(d)
        },
    }
}

const cabin: PropFactory = (s) => {
    const CAB = ['      __    ', '  /\\  |  |  ', ' /  \\_|__|  ', '/______\\    ', '| [##] |    ', '|______|    ']
    let smoke: Smoke[] = [], nextPuff = 0
    return {
        init() { smoke = []; nextPuff = 0 },
        draw(d, t) {
            const c0 = Math.round(s.cols * (s.layout.kind === 'chat' ? .74 : .5)) - 6, r0 = s.floorRow() - 5
            s.art(CAB, r0, c0, 5, {'#': 4, '[': 4, ']': 4})
            if (t >= nextPuff) { smoke.push({x: c0 + 7.5, y: r0 - 1, a: 0, ph: rand() * 6}); nextPuff = t + 1.1 + rand() * .8 }
            for (let i = smoke.length - 1; i >= 0; i--) {
                const p = smoke[i]
                p.a += d / 8; p.y -= 1.25 * d; p.x += (.6 + Math.sin(t * .6 + p.ph) * .5) * d
                if (p.a > 1) { smoke.splice(i, 1); continue }
                s.put(Math.floor(p.y), Math.floor(p.x), p.a < .3 ? 'O' : p.a < .6 ? 'o' : p.a < .85 ? '.' : '·', p.a < .6 ? 3 : p.a < .85 ? 2 : 1)
            }
        },
    }
}

const train: PropFactory = (s) => {
    const LOCO = ['   _     ', ' _| |__  ', '(o_o_o)> '], CAR = [' ______ ', '|[][][]|', '(o)--(o)']
    const rowsT = [0, 1, 2].map(i => CAR[i] + ' ' + CAR[i] + ' ' + CAR[i] + ' ' + LOCO[i])
    const len = rowsT[0].length, stackAt = len - 9 + 3
    let x = -999, next = 3, smoke: Omit<Smoke, 'ph'>[] = [], nextPuff = 0
    return {
        init() { x = -999; next = 3; smoke = [] },
        draw(d, t) {
            const base = s.floorRow()
            for (let c = 0; c < s.cols; c++) s.put(base + 1, c, c % 4 === 0 ? '+' : '-', 1)
            if (x < -len - 2 && t >= next) x = -len
            if (x > -len - 1 && x < s.cols + 2) {
                x += 6 * d
                rowsT.forEach((row, i) => { for (let k = 0; k < row.length; k++) if (row[k] !== ' ') s.put(base - 2 + i, Math.floor(x) + k, row[k], '[]'.includes(row[k]) ? 4 : 3) })
                if (t >= nextPuff) { smoke.push({x: x + stackAt, y: base - 3, a: 0}); nextPuff = t + .55 }
                if (x > s.cols + 1) { x = -999; next = t + 35 + rand() * 40 }
            }
            for (let i = smoke.length - 1; i >= 0; i--) {
                const p = smoke[i]
                p.a += d / 3.2; p.y -= 1.6 * d; p.x -= 2.2 * d
                if (p.a > 1) { smoke.splice(i, 1); continue }
                s.put(Math.floor(p.y), Math.floor(p.x), p.a < .35 ? 'o' : p.a < .7 ? '.' : '·', p.a < .35 ? 3 : p.a < .7 ? 2 : 1)
            }
        },
    }
}

export const PROPS = {sleepingCat, owl, campfire, cabin, train} satisfies Record<string, PropFactory>
export type PropName = keyof typeof PROPS

export interface SceneOptions {
    stars?: boolean
    fireflies?: boolean
    props: PropName[]
}

/** A cozy scene: a night sky or fireflies behind, little props on the floor above the composer */
export const scene = ({stars = false, fireflies = false, props}: SceneOptions): EffectFactory => (s): Effect => {
    const sky = stars ? new StarsLayer(s, {gap: [9, 20], density: .025}) : null
    const ff = fireflies ? new FirefliesLayer(s, {density: .022}) : null
    const parts = props.map(name => PROPS[name](s))
    return {
        init() { sky?.init(); ff?.init(); parts.forEach(p => p.init()) },
        draw(d, t) { s.begin(); sky?.draw(d, t); ff?.put(d, t); parts.forEach(p => p.draw(d, t)); s.flush() },
    }
}

