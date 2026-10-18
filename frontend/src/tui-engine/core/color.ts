import type {Rgb, ThemeColors} from './types.ts'

export const rgba = (c: Rgb, a: number): string => `rgba(${c[0]},${c[1]},${c[2]},${a})`

/** Reads a CSS color as RGB through the canvas, which accepts any CSS color syntax and hands back `#rrggbb` */
export function parseColor(ctx: CanvasRenderingContext2D, value: string): Rgb {
    ctx.fillStyle = '#000'
    ctx.fillStyle = value
    const hex = String(ctx.fillStyle)
    if (!hex.startsWith('#')) return [0, 0, 0]
    return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as Rgb
}

/** The theme's three colors as a style resolves them: `--color-secondary` is the page, `--color-tertiary` the accent
 * and `--color-primary` the text */
export function readColors(ctx: CanvasRenderingContext2D, style: CSSStyleDeclaration): ThemeColors {
    return {
        bg: parseColor(ctx, style.getPropertyValue('--color-secondary').trim()),
        accent: parseColor(ctx, style.getPropertyValue('--color-tertiary').trim()),
        fg: parseColor(ctx, style.getPropertyValue('--color-primary').trim()),
    }
}

// How a theme's accent stands out from its background, as CIELAB lightness (0-100, close to how the eye sees it)
const lightness = (c: Rgb): number => {
    const lin = c.map((v) => { const x = v / 255; return x <= .04045 ? x / 12.92 : ((x + .055) / 1.055) ** 2.4 })
    const y = .2126 * lin[0] + .7152 * lin[1] + .0722 * lin[2]
    return y > 216 / 24389 ? 116 * Math.cbrt(y) - 16 : y * 24389 / 27
}
/** The lightness gap between accent and background the effects were tuned on (the dark themes, around Dusk) */
const TUNED_GAP = 55
/** Thin dark strokes on a light page read weaker than thin light strokes on a dark one, at the same gap */
const LIGHT_PAGE_BOOST = 1.35

/** How much stronger the ink must be on a theme for the effects to show as they do on the themes they were tuned on:
 * 1 on those, more where the accent is closer to the background or the page is light */
export function inkFor(bg: Rgb, accent: Rgb): number {
    const page = lightness(bg), gap = Math.max(8, Math.abs(lightness(accent) - page))
    return Math.min(3, Math.max(1, TUNED_GAP / gap) * (page > 50 ? LIGHT_PAGE_BOOST : 1))
}
