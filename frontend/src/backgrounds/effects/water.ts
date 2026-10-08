import {rand} from '../engine/random.ts'
import type {Effect, EffectFactory, Level} from '../engine/types.ts'
import {FirefliesLayer} from '../layers/fireflies.ts'

/** A sprite and its mirror image, for swimming the other way */
type Sprite = [string, string]

const MIRRORED: Record<string, string> = {'>': '<', '<': '>', '(': ')', ')': '(', '/': '\\', '\\': '/', '{': '}', '}': '{'}
const mirror = (s: string): string => [...s].reverse().map(ch => MIRRORED[ch] || ch).join('')
const FISH: Sprite[] = ['><>', '><(((o>', '}=>', '>=(o>', '><))>', '>>-(o)>'].map(s => [s, mirror(s)])

interface Fish { x: number; y: number; dir: number; v: number; s: Sprite; l: Level; rowFor: () => number }
interface Bubble { x: number; y: number; v: number; ph: number; c: string }
interface Weed { x: number; h: number; ph: number }

/** An aquarium filling the window: a surface, sand above the composer, weeds, fish, a crab and bubbles */
export const aquarium = (): EffectFactory => (s): Effect => {
    let fish: Fish[] = [], bubbles: Bubble[] = [], weeds: Weed[] = [], crab: { x: number; dir: number } | null = null, top = 0, bot = 0, sand: number[] = []
    return {
        init() {
            bot = s.floorRow(); top = 2
            const depth = bot - top, rowFor = () => top + 3 + Math.floor(rand() * Math.max(1, depth - 6))
            fish = Array.from({length: Math.max(9, Math.floor(s.cols * depth / 300))}, () => {
                const sp = FISH[Math.floor(rand() * FISH.length)], dir = rand() < .5 ? 1 : -1
                return {x: rand() * s.cols, y: rowFor(), dir, v: 1.2 + rand() * 3, s: sp, l: (2 + Math.floor(rand() * 2)) as Level, rowFor}
            })
            bubbles = Array.from({length: Math.floor(s.cols / 7)}, () => ({x: rand() * s.cols, y: top + rand() * (bot - top), v: 1.2 + rand() * 1.6, ph: rand() * 6, c: rand() < .7 ? 'o' : 'O'}))
            weeds = Array.from({length: Math.floor(s.cols / 9)}, () => ({x: Math.floor(rand() * s.cols), h: 4 + Math.floor(rand() * Math.max(2, depth * .35)), ph: Math.floor(rand() * 4)}))
            sand = Array.from({length: s.cols}, (_, c) => Math.max(0, Math.floor(1.2 + Math.sin(c * .06) * .8 + Math.sin(c * .17) * .45)))
            crab = {x: rand() * s.cols, dir: rand() < .5 ? 1 : -1}
        },
        draw(d, t) {
            s.begin()
            const {cols} = s
            const sh = Math.floor(t * 1.1)
            for (let c = 0; c < cols; c++) { s.put(top, c, ((c + sh) % 6 < 3) ? '~' : '-', 1); s.put(top + 1, c, ((c - sh + 3) % 8 < 3) ? '~' : ' ', 1) }
            for (let c = 0; c < cols; c++) {
                s.put(bot, c, ':', 1)
                for (let k = 1; k <= sand[c]; k++) s.put(bot - k, c, k === sand[c] ? '.' : ':', 1)
            }
            for (const w of weeds) for (let h = 0; h < w.h; h++) s.put(bot - sand[Math.min(cols - 1, w.x)] - 1 - h, w.x, ((h + w.ph + Math.floor(t * .7)) % 2) ? '(' : ')', 2)
            for (const f of fish) {
                f.x += f.dir * f.v * d
                const sp = f.dir > 0 ? f.s[0] : f.s[1]
                if (f.dir > 0 && f.x > cols + 2) { f.x = -sp.length - 2; f.y = f.rowFor() }
                if (f.dir < 0 && f.x < -sp.length - 2) { f.x = cols + 2; f.y = f.rowFor() }
                const x = Math.floor(f.x)
                for (let i = 0; i < sp.length; i++) s.put(f.y, x + i, sp[i], f.l)
            }
            if (crab) {
                crab.x += crab.dir * 1.6 * d
                if (crab.x < 2 || crab.x > cols - 8) crab.dir *= -1
                const sp = Math.floor(t * 1.6) % 2 ? '\\(oo)/' : '/(oo)\\', y = bot - 1 - sand[Math.min(cols - 1, Math.max(0, Math.floor(crab.x)))]
                for (let i = 0; i < sp.length; i++) s.put(y, Math.floor(crab.x) + i, sp[i], 3)
            }
            for (const b of bubbles) {
                b.y -= b.v * d
                if (b.y <= top + 1) { b.y = bot - 1; b.x = rand() * cols }
                s.putIfEmpty(Math.floor(b.y), Math.floor(b.x + Math.sin(t * .8 + b.ph) * 1.2), b.c, b.y < top + 3 ? 1 : 2)
            }
            s.flush()
        },
    }
}

interface Jelly { big: boolean; x: number; y: number; v: number; ph: number; l: Level }

/** The deep sea: jellyfish slowly rising and pulsing, glowing plankton, bubbles. No fish, quieter than the aquarium */
export const deepSea = (): EffectFactory => (s): Effect => {
    const BIG = [['  .-.  ', ' (   ) ', '  |||  ', ' ) | ( '], ['  .-.  ', ' (___) ', '  |||  ', '  (|)  ']]
    const SMALL = [[' .-. ', '(   )', ' ))( '], [' .-. ', '(___)', ' (() ']]
    let js: Jelly[] = [], bubbles: Omit<Bubble, 'c'>[] = []
    const plankton = new FirefliesLayer(s, {density: .02, per: [4, 9], chars: ['·', '·', '•', 'o']})
    return {
        init() {
            plankton.init()
            js = Array.from({length: Math.max(5, Math.floor(s.cols / 26))}, () => ({big: rand() < .55, x: rand() * s.cols, y: rand() * s.rows, v: .35 + rand() * .6, ph: rand() * 6, l: (3 + (rand() < .4 ? 1 : 0)) as Level}))
            bubbles = Array.from({length: Math.floor(s.cols / 16)}, () => ({x: rand() * s.cols, y: rand() * s.rows, v: .9 + rand() * 1.2, ph: rand() * 6}))
        },
        draw(d, t) {
            s.begin()
            plankton.put(d, t)
            for (const b of bubbles) {
                b.y -= b.v * d
                if (b.y < -1) { b.y = s.rows; b.x = rand() * s.cols }
                s.putIfEmpty(Math.floor(b.y), Math.floor(b.x + Math.sin(t * .7 + b.ph) * 1.1), '·', 1)
            }
            for (const j of js) {
                const fr = (j.big ? BIG : SMALL)[Math.floor(t * .9 + j.ph) % 2]
                j.y -= j.v * (.6 + .4 * Math.sin(t * 1.8 + j.ph)) * d
                if (j.y < -5) { j.y = s.rows + 2; j.x = rand() * s.cols }
                s.art(fr, Math.floor(j.y), Math.floor(j.x + Math.sin(t * .3 + j.ph) * 3), j.l)
            }
            s.flush()
        },
    }
}
