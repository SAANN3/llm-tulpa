import type {TuiDimension} from './types.ts'

/** A length in cells: a number, `'120px'`, `'50%'`, or a share with an offset (`'100% - 2'`, `'50% + 1'`); null when the
 * text is none of these */
function parseLength(val: TuiDimension, parentSize: number, cellPx: number): number | null {
    if (typeof val === 'number') return Number.isFinite(val) ? Math.round(val) : null
    const s = val.trim().toLowerCase()

    if (s.endsWith('px')) {
        const px = parseFloat(s)
        return Number.isFinite(px) && cellPx > 0 ? Math.round(px / cellPx) : null
    }

    const share = s.match(/^(\d+(?:\.\d+)?)%\s*(?:([+-])\s*(\d+(?:\.\d+)?))?$/)
    if (share) {
        const offset = share[2] ? (share[2] === '-' ? -1 : 1) * parseFloat(share[3]) : 0
        return Math.round(parseFloat(share[1]) / 100 * parentSize + offset)
    }

    const plain = Number(s)
    return s !== '' && Number.isFinite(plain) ? Math.round(plain) : null
}

/** A width or height in cells, never negative; null when `val` can't be read, which the caller treats as not given */
export function resolveSize(val: TuiDimension, parentSize: number, cellPx: number): number | null {
    const n = parseLength(val, parentSize, cellPx)
    return n === null ? null : Math.max(0, n)
}

/** A column or row offset in cells inside a parent of `parentSize`; `'center'` puts a widget of `selfSize` in the middle.
 * Null when `val` can't be read. */
export function resolveOffset(val: TuiDimension, parentSize: number, selfSize: number, cellPx: number): number | null {
    if (typeof val === 'string' && val.trim().toLowerCase() === 'center') return Math.max(0, Math.floor((parentSize - selfSize) / 2))
    return parseLength(val, parentSize, cellPx)
}
