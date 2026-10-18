// The grid has one character per cell, but text doesn't: an emoji or a CJK character is one symbol that fills two cells,
// and a letter with an accent can be two code points. Everything that measures or cuts text goes through here, so a
// wide symbol is never split and never overlaps its neighbour.

const segmenter = typeof Intl.Segmenter === 'function' ? new Intl.Segmenter(undefined, {granularity: 'grapheme'}) : null

/** The symbols the user sees, one entry each (an emoji with a skin tone is one) */
export const graphemes = (text: string): string[] =>
    segmenter ? Array.from(segmenter.segment(text), (s) => s.segment) : Array.from(text)

/** The East Asian Wide and Fullwidth blocks, which monospace fonts draw two cells wide */
const WIDE: [number, number][] = [
    [0x1100, 0x115F], [0x2E80, 0x303E], [0x3041, 0x33FF], [0x3400, 0x4DBF], [0x4E00, 0x9FFF], [0xA000, 0xA4CF],
    [0xA960, 0xA97F], [0xAC00, 0xD7A3], [0xF900, 0xFAFF], [0xFE30, 0xFE6F], [0xFF00, 0xFF60], [0xFFE0, 0xFFE6],
    [0x1F300, 0x1F64F], [0x1F900, 0x1F9FF], [0x20000, 0x3FFFD],
]
const EMOJI = /\p{Emoji_Presentation}|️/u

/** How many cells a symbol fills */
export function cellWidth(g: string): 1 | 2 {
    const cp = g.codePointAt(0) ?? 0
    // Latin, Cyrillic and the rest of the narrow scripts: the common case, settled without a lookup
    if (cp < 0x1100 && g.length === 1) return 1
    if (EMOJI.test(g)) return 2
    return WIDE.some(([from, to]) => cp >= from && cp <= to) ? 2 : 1
}

/** How many cells a text fills on one line */
export const textWidth = (text: string): number => graphemes(text).reduce((sum, g) => sum + cellWidth(g), 0)

/** The longest start of `line` that fits `cols` cells */
export function clipLine(line: string, cols: number): string {
    let width = 0
    let out = ''
    for (const g of graphemes(line)) {
        width += cellWidth(g)
        if (width > cols) break
        out += g
    }
    return out
}

/** `line` cut to `cols` cells, ending in `…` when something was cut */
export function truncateLine(line: string, cols: number): string {
    if (textWidth(line) <= cols) return line
    return cols > 1 ? clipLine(line, cols - 1) + '…' : clipLine(line, cols)
}

/** `line` broken into pieces of at most `cols` cells, at a space where there is one */
export function wrapLine(line: string, cols: number): string[] {
    const gs = graphemes(line)
    // An empty line is still a line: a blank row between two paragraphs
    if (!gs.length) return ['']
    const out: string[] = []
    let start = 0
    while (start < gs.length) {
        let width = 0
        let end = start
        while (end < gs.length && width + cellWidth(gs[end]) <= cols) width += cellWidth(gs[end++])
        if (end === gs.length) {
            out.push(gs.slice(start).join(''))
            break
        }
        // A symbol wider than the room still gets a line of its own, so the loop moves on
        if (end === start) end++
        // The piece ends at the last space it contains, unless the next symbol is the space itself
        let cut = end
        if (gs[end] !== ' ') {
            for (let k = end - 1; k > start; k--) if (gs[k] === ' ') { cut = k; break }
        }
        out.push(gs.slice(start, cut).join(''))
        start = gs[cut] === ' ' ? cut + 1 : cut
    }
    return out
}

/** The lines `text` takes in `cols` cells: its own line breaks, then each line wrapped, cut with `…`, or cut off */
export function formatTextLines(text: string, cols: number | undefined, mode: 'wrap' | 'truncate' | 'clip'): string[] {
    if (!text) return []
    return text.split('\n').flatMap((line) => {
        if (cols === undefined) return [line]
        if (mode === 'wrap') return wrapLine(line, cols)
        return [mode === 'truncate' ? truncateLine(line, cols) : clipLine(line, cols)]
    })
}
