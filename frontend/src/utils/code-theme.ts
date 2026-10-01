import type {CSSProperties} from 'react'

// A Prism token theme made only of the active theme's own colors, so highlighted code follows
// whichever theme is selected instead of carrying a palette of its own. The editor-style
// distinctions come from mixing the two colors a theme already has: its text (primary) and its
// accent (tertiary). `react-syntax-highlighter` writes these straight into inline styles, which
// is why they're CSS values and not classes.
const text = 'var(--color-primary)'
const accent = 'var(--color-tertiary)'
const mix = (color: string, percent: number, other = 'transparent') =>
    `color-mix(in srgb, ${color} ${percent}%, ${other})`

const keyword: CSSProperties = {color: accent}
const literal: CSSProperties = {color: mix(accent, 60, text)}
const name: CSSProperties = {color: mix(accent, 30, text)}
const quiet: CSSProperties = {color: mix(text, 70)}
const comment: CSSProperties = {color: mix(text, 50), fontStyle: 'italic'}

export const codeTheme: Record<string, CSSProperties> = {
    'code[class*="language-"]': {color: text, background: 'none', fontFamily: 'inherit', whiteSpace: 'pre'},
    // No background or padding here: the block keeps the page's own code-block box (`.markdown pre`)
    'pre[class*="language-"]': {color: text, fontFamily: 'inherit', overflow: 'auto'},
    comment,
    prolog: comment,
    doctype: comment,
    cdata: comment,
    punctuation: quiet,
    operator: quiet,
    namespace: quiet,
    keyword,
    atrule: keyword,
    tag: keyword,
    selector: keyword,
    builtin: keyword,
    important: {...keyword, fontWeight: 'bold'},
    string: literal,
    char: literal,
    'attr-value': literal,
    regex: literal,
    url: literal,
    inserted: literal,
    number: name,
    boolean: name,
    constant: name,
    symbol: name,
    property: name,
    'attr-name': name,
    variable: name,
    entity: name,
    function: {color: text, fontWeight: 'bold'},
    'class-name': {color: text, fontWeight: 'bold'},
    deleted: {color: 'var(--color-danger)'},
    bold: {fontWeight: 'bold'},
    italic: {fontStyle: 'italic'},
};
