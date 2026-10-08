import {clamp, hash, mulberry, rand} from '../engine/random.ts'
import type {Effect, EffectFactory, Level} from '../engine/types.ts'

/** One character of a tree: where it sits relative to the pot's top centre, its level, the foliage pad it sways
 * with (-1 for wood), and its order in the growth */
interface Cell { r: number; c: number; ch: string; l: Level; pad: number; o: number }
interface Pad { phase: number; amp?: number }
interface Tree { cells: Cell[]; pads: Pad[]; maxOrd: number }

const PALETTE = ['&', '@', '%', '*', '~', 'o', '#', '8']
const PALETTE_AUTUMN = ['~', '&', '%', '*', 'o', '8', '#', '@']

/** A small tree: a slender S-curved trunk, three limbs with open space between them and an apex pad */
function buildBonsai(seed: number, autumn: boolean): Tree {
    const rnd = mulberry(seed), cells: Cell[] = [], pads: Pad[] = []
    let ord = 0
    const add = (r: number, c: number, ch: string, l: Level, pad: number, o: number) => cells.push({r, c, ch, l, pad, o})

    // pot
    const w = 28, potTop = '.' + '='.repeat(w) + '.', potMid = '|' + ' . '.repeat(9) + ' ' + '|', potBot = '\\' + '_'.repeat(w) + '/'
    const feet = '   |___|' + ' '.repeat(w - 14) + '|___|';
    [potTop, potMid, potBot, feet].forEach((s, r) => { for (let i = 0; i < s.length; i++) if (s[i] !== ' ') add(r, i - 15, s[i], 4, -1, 0) })

    // slender S-curved trunk
    const th = 13 + Math.floor(rnd() * 4), dir = rnd() < .5 ? -1 : 1, amp = 2 + rnd() * 1.6, lean = (rnd() - .5) * 3
    const tx: number[] = []
    for (let h = 0; h < th; h++) tx.push(Math.round(Math.sin(h / th * Math.PI * 1.5) * amp * dir + lean * h / th))
    for (let h = 0; h < th; h++) {
        const r = -1 - h, x = tx[h], dx = (h < th - 1 ? tx[h + 1] : x) - x, wd = h < 3 ? 3 : h < 8 ? 2 : 1
        ord++
        if (wd === 3) { add(r, x - 1, '/', 5, -1, ord); add(r, x, '#', 5, -1, ord); add(r, x + 1, '\\', 5, -1, ord) }
        else if (wd === 2) { add(r, x, dx < 0 ? '\\' : '#', 5, -1, ord); add(r, x + 1, dx > 0 ? '/' : '#', 5, -1, ord) }
        else add(r, x, dx > 0 ? '/' : dx < 0 ? '\\' : '|', 5, -1, ord)
        if (wd < 3 && Math.abs(dx) > 1) add(r, x + Math.sign(dx), '_', 5, -1, ord)
    }

    const pal = autumn ? PALETTE_AUTUMN : PALETTE
    const addPad = (cr: number, cc: number, rc: number, rr: number, idx: number) => {
        const base = ord
        pads.push({phase: idx * 1.7 + rnd() * 2})
        for (let dr = -rr; dr <= rr; dr++) for (let dc = -rc; dc <= rc; dc++) {
            const dd = (dr / rr) ** 2 + (dc / rc) ** 2, rag = 1 + (hash(dr, dc, idx, seed) - .5) * .42
            if (dd > rag) continue
            const hh = hash(dc, dr, idx + 9, seed), ch = pal[Math.floor(hh * pal.length)]
            const l: Level = dr < 0 ? 3 : (hh > .93 ? 4 : 2)
            add(cr + dr, cc + dc, ch, l, idx, base + Math.floor(dd * 9))
        }
        ord += 11
    }

    // three lateral limbs with open space between them, then an apex pad
    const lh = [Math.floor(th * .36), Math.floor(th * .62), th - 2], lens = [12, 10, 7]
    lh.forEach((hgt, i) => {
        let c = tx[hgt], r = -1 - hgt
        const s = i % 2 === 0 ? -dir : dir, len = lens[i] + Math.floor(rnd() * 3)
        for (let l = 0; l < len; l++) {
            c += s
            const rise = l % 3 === 1
            if (rise) r -= 1
            ord++
            add(r, c, rise ? (s > 0 ? '/' : '\\') : '-', 5, -1, ord)
        }
        addPad(r - 1, c, 7 - i, 2, i)
    })
    addPad(-1 - th - 2, tx[th - 1], 9, 3, 3)
    return {cells, pads, maxOrd: ord + 4}
}

/** A big tree: fills ~80% of the height and up to ~50% of the working width, with room around it */
function buildBonsaiGrand(seed: number, autumn: boolean, Ht: number, Wd: number): Tree {
    const rnd = mulberry(seed), cells: Cell[] = [], pads: Pad[] = []
    const add = (r: number, c: number, ch: string, l: Level, pad: number, o: number) => cells.push({r, c, ch, l, pad, o})
    const pal = autumn ? PALETTE_AUTUMN : PALETTE
    const halfW = Math.floor(Wd / 2)

    // pot
    const pw = clamp(Math.round(Wd * 0.5), 30, 50), inner = pw - 2, ox = Math.floor(pw / 2)
    let mid = '|'
    for (let i = 0; i < inner; i++) mid += (i % 3 === 1) ? '.' : ' '
    mid += '|'
    const potLines = ['.' + '='.repeat(inner) + '.', mid, '\\' + '_'.repeat(inner) + '/', '   |___|' + ' '.repeat(Math.max(1, pw - 16)) + '|___|']
    potLines.forEach((s, r) => { for (let i = 0; i < s.length; i++) if (s[i] !== ' ') add(r, i - ox, s[i], 4, -1, 0) })

    // trunk: flared root, tapering, S-curve
    const apexRR = clamp(Math.round(Ht * 0.07), 3, 5), apexRC = clamp(Math.round(Wd * 0.15), 9, 18)
    const th = Math.max(12, Ht - apexRR * 2 - 2)
    const dir = rnd() < .5 ? -1 : 1, amp = clamp(2 + th * 0.1, 3, 7), lean = (rnd() - .5) * 4
    const tx: number[] = [], ordT: number[] = []
    for (let h = 0; h < th; h++) { tx.push(Math.round(Math.sin(h / th * Math.PI * 1.5) * amp * dir + lean * h / th)); ordT.push(2 * h + 2) }
    for (let h = 0; h < th; h++) {
        const r = -1 - h, x = tx[h], dx = (h < th - 1 ? tx[h + 1] : x) - x, f = h / th
        const wd = h < 2 ? 7 : f < .18 ? 5 : f < .38 ? 4 : f < .6 ? 3 : f < .82 ? 2 : 1
        const x0 = x - Math.floor(wd / 2), o = ordT[h]
        if (wd >= 3) for (let i = 0; i < wd; i++) add(r, x0 + i, i === 0 ? '/' : i === wd - 1 ? '\\' : '#', 5, -1, o)
        else if (wd === 2) { add(r, x, dx < 0 ? '\\' : '#', 5, -1, o); add(r, x + 1, dx > 0 ? '/' : '#', 5, -1, o) }
        else add(r, x, dx > 0 ? '/' : dx < 0 ? '\\' : '|', 5, -1, o)
        if (wd < 3 && Math.abs(dx) > 1) add(r, x + Math.sign(dx), '_', 5, -1, o)
    }

    // foliage pads: soft edges (dim), brighter tops, a few bright specks
    let padIdx = 0
    const addPad = (cr: number, cc: number, rc: number, rr: number, baseOrd: number, swayAmp: number) => {
        const idx = padIdx++
        pads.push({phase: idx * 1.7 + rnd() * 2, amp: swayAmp})
        for (let dr = -rr; dr <= rr; dr++) for (let dc = -rc; dc <= rc; dc++) {
            const dd = (dr / rr) ** 2 + (dc / rc) ** 2, rag = 1 + (hash(dr, dc, idx, seed) - .5) * .42
            if (dd > rag) continue
            const hh = hash(dc, dr, idx + 9, seed), ch = pal[Math.floor(hh * pal.length)]
            const l: Level = hh > .95 ? 4 : dd > .8 ? 1 : (dr < 0 ? 3 : 2)
            add(cr + dr, cc + dc, ch, l, idx, baseOrd + Math.floor(dd * 12))
        }
    }

    // limbs: lower ones long, upper ones short; alternate sides; some get a twig with a small pad
    const n = clamp(Math.round(th / 7), 3, 6)
    for (let i = 0; i < n; i++) {
        const frac = n === 1 ? 0 : i / (n - 1)
        const hgt = Math.min(th - 2, Math.round(th * 0.26 + th * 0.64 * frac))
        const s = i % 2 === 0 ? -dir : dir
        const want = halfW * (1.0 - 0.5 * frac) * (0.8 + rnd() * .35)
        const rc = clamp(Math.round(want * 0.34), 6, 13)
        const len = Math.max(5, Math.min(Math.round(want), halfW - rc - s * tx[hgt]))
        const per = clamp(Math.round(len / 4), 3, 6)
        let c = tx[hgt], r = -1 - hgt, o = ordT[hgt]
        const twigAt = len >= 16 ? Math.floor(len * 0.55) : -1
        let twig: { r: number; c: number; o: number } | null = null
        for (let l = 0; l < len; l++) {
            c += s
            const rise = l % per === per - 1
            if (rise) r -= 1
            o += 1.4
            add(r, c, rise ? (s > 0 ? '/' : '\\') : '-', 5, -1, o)
            if (l === twigAt) twig = {r, c, o}
        }
        const sa = 0.8 + 1.4 * (hgt / th)
        addPad(r - 1, c, rc, frac < .5 ? 3 : 2, o + 1, sa)
        if (twig) { for (let t = 1; t <= 3; t++) add(twig.r - t, twig.c, '|', 5, -1, twig.o + t); addPad(twig.r - 5, twig.c, 5, 2, twig.o + 4, sa) }
    }
    addPad(-th - apexRR + 1, tx[th - 1], apexRC, apexRR, ordT[th - 1], 1.8)

    let maxOrd = 0
    for (const c of cells) if (c.o > maxOrd) maxOrd = c.o
    return {cells, pads, maxOrd: maxOrd + 2}
}

export interface BonsaiOptions {
    autumn?: boolean
    /** The tree grows from the pot up, then the leaves start falling */
    grow?: boolean
    /** The big tree */
    grand?: boolean
}

interface Leaf { x: number; y: number; vx: number; vy: number; c: number }
interface Placed { cx: number; tree: Tree }

/** A bonsai standing above the composer, its pads swaying, leaves drifting across the window. One tree in the
 * middle of a chat page, two on the sides of a centered page. */
export const bonsai = ({autumn = false, grow = false, grand = false}: BonsaiOptions): EffectFactory => (s): Effect => {
    let trees: Placed[] = [], t0 = 0, leaves: Leaf[] = [], seedBase = 0
    const GROW = grand ? 80 : 42

    function layoutTrees(): Placed[] {
        const base = s.floorRow()
        if (!grand) {
            if (s.layout.kind === 'chat') return [{cx: s.chatCenterCol(), tree: buildBonsai(seedBase, autumn)}]
            return [{cx: Math.round(s.W * .13 / s.cw), tree: buildBonsai(seedBase, autumn)}, {cx: Math.round(s.W * .87 / s.cw), tree: buildBonsai(seedBase + 7919, autumn)}]
        }
        const Ht = Math.min(Math.floor(s.rows * 0.74), base - 4)
        if (s.layout.kind === 'chat') {
            const areaCols = Math.floor((s.W - s.layout.sidebarPx) / s.cw), Wd = Math.max(40, Math.floor(areaCols * 0.5))
            return [{cx: s.chatCenterCol(), tree: buildBonsaiGrand(seedBase, autumn, Ht, Wd)}]
        }
        const side = s.sideCols(), Wd = Math.max(30, Math.floor(side * 0.92))
        return [{cx: Math.round(side / 2), tree: buildBonsaiGrand(seedBase, autumn, Ht, Wd)}, {cx: s.cols - Math.round(side / 2) - 1, tree: buildBonsaiGrand(seedBase + 7919, autumn, Ht, Wd)}]
    }

    return {
        init(reseed) {
            if (reseed || !seedBase) seedBase = Math.floor(rand() * 99999) + 1
            trees = layoutTrees()
            t0 = performance.now() / 1000
            const nl = grand ? Math.floor(s.cols * s.rows / 95) : (autumn ? 40 : 26)
            leaves = Array.from({length: nl}, () => ({x: rand() * s.cols, y: rand() * s.rows, vx: -(0.6 + rand() * 1.4), vy: 0.5 + rand() * 1.0, c: rand()}))
        },
        draw(d, t) {
            s.begin()
            const base = s.floorRow()
            const prog = grow ? clamp((performance.now() / 1000 - t0) / GROW, 0, 1) : 1, done = prog >= 1
            for (const tr of trees) {
                const reveal = prog * tr.tree.maxOrd, sw = tr.tree.pads.map(p => Math.round(Math.sin(t * 0.42 + p.phase) * (p.amp || 1.2)))
                for (const c of tr.tree.cells) {
                    if (c.o > reveal) continue
                    s.put(base + c.r, tr.cx + c.c + (c.pad >= 0 ? sw[c.pad] : 0), c.ch, c.l)
                }
            }
            if (done || !grow) {
                const pal = autumn ? ['~', '*', '%', '&', '.'] : ['~', '*', '.', '^', 'o']
                for (const lf of leaves) {
                    lf.x += lf.vx * d; lf.y += lf.vy * d
                    if (lf.x < 0 || lf.y >= s.rows) { lf.x = s.cols + rand() * 30; lf.y = rand() * s.rows * .6 - 4 }
                    s.putIfEmpty(Math.floor(lf.y), Math.floor(lf.x), pal[Math.floor(lf.c * pal.length)], lf.c > .6 ? 2 : 1)
                }
            }
            s.flush()
        },
    }
}
