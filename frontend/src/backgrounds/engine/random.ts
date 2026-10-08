// Two kinds of randomness. `rand` is for what may change every time (where a firefly appears next). `mulberry` and
// `hash` are deterministic: a tree's leaves or a mountain's outline are built from them, so they come out the same
// on every frame instead of flickering.

export const rand = Math.random

/** A seeded generator: the same seed gives the same sequence */
export function mulberry(seed: number): () => number {
    let a = seed
    return () => {
        a |= 0; a = a + 0x6D2B79F5 | 0
        let t = Math.imul(a ^ a >>> 15, 1 | a)
        t = t + Math.imul(t ^ t >>> 7, 61 | t) ^ t
        return ((t ^ t >>> 14) >>> 0) / 4294967296
    }
}

/** A stable value in [0, 1) for a cell (a, b), a layer c and a seed */
export function hash(a: number, b: number, c: number, seed: number): number {
    let x = Math.imul((a * 73856093) ^ (b * 19349663) ^ (c * 83492791) ^ seed, 0x45d9f3b)
    x ^= x >>> 13; x = Math.imul(x, 0x45d9f3b); x ^= x >>> 15
    return (x >>> 0) / 4294967296
}

export const clamp = (x: number, a: number, b: number): number => Math.max(a, Math.min(b, x))

export function smooth(a: number, b: number, x: number): number {
    const t = clamp((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)
}

// Smooth 3D value noise; its table is shuffled once per page load
const perm = new Uint8Array(512)
{
    const p = [...Array(256).keys()]
    for (let i = 255; i > 0; i--) { const j = (rand() * (i + 1)) | 0; [p[i], p[j]] = [p[j], p[i]] }
    for (let i = 0; i < 512; i++) perm[i] = p[i & 255]
}
const fade = (t: number): number => t * t * (3 - 2 * t)

/** The noise's own lattice value at an integer point, in [0, 1] */
export const h3 = (x: number, y: number, z: number): number => perm[(perm[(perm[x & 255] + y) & 255] + z) & 255] / 255

export function noise(x: number, y: number, z: number): number {
    const xi = Math.floor(x), yi = Math.floor(y), zi = Math.floor(z), xf = fade(x - xi), yf = fade(y - yi), zf = fade(z - zi)
    const l = (a: number, b: number, t: number) => a + (b - a) * t, c = (dx: number, dy: number, dz: number) => h3(xi + dx, yi + dy, zi + dz)
    return l(l(l(c(0, 0, 0), c(1, 0, 0), xf), l(c(0, 1, 0), c(1, 1, 0), xf), yf), l(l(c(0, 0, 1), c(1, 0, 1), xf), l(c(0, 1, 1), c(1, 1, 1), xf), yf), zf)
}
