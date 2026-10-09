const LEAF = 'M0,0 C3,-7 9,-5 12,0 C8,1 3,4 0,0 Z'

/** How long each of the two faces shows while there is news: the icon with a dot, then the number */
const ALTERNATE_MS = 2000

const leaves = (accent: string) =>
    `<g fill="${accent}" transform="translate(16,16)">` +
    [0, 90, 180, 270].map((angle) => `<path d="${LEAF}" transform="rotate(${angle})"/>`).join('') +
    `</g>`;

// The same icon as `public/favicon.svg`, in the given colors; with news, the dot in its corner
const iconSvg = (background: string, accent: string, dot: boolean) =>
    `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">` +
    `<rect width="32" height="32" rx="7" fill="${background}"/>` + leaves(accent) +
    (dot ? `<circle cx="25" cy="7" r="6.5" fill="${accent}" stroke="${background}" stroke-width="2.5"/>` : '') +
    `</svg>`;

// The number of chats with news in place of the leaves. A digit's visible middle is half its height (about 0.36 of
// the font size) above the baseline, so the baseline goes that far below the icon's middle: "3" and the smaller "9+"
// sit at the same height.
const countSvg = (background: string, accent: string, count: number) => {
    const text = count > 9 ? '9+' : String(count)
    const size = count > 9 ? 15 : 21
    return `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">` +
        `<rect width="32" height="32" rx="7" fill="${background}"/>` +
        `<text x="16" y="${(16 + 0.36 * size).toFixed(1)}" text-anchor="middle" font-family="monospace" font-weight="700" font-size="${size}" fill="${accent}">${text}</text>` +
        `</svg>`
};

let unread = 0
let showCount = false
let timer: number | undefined

const draw = () => {
    const styles = getComputedStyle(document.documentElement)
    const background = styles.getPropertyValue('--color-secondary').trim()
    const accent = styles.getPropertyValue('--color-tertiary').trim()
    const link = document.querySelector<HTMLLinkElement>('link[rel="icon"]')
    // Keeps the static icon when the theme's colors aren't available yet
    if (!link || !background || !accent) return

    const svg = unread > 0 && showCount ? countSvg(background, accent, unread) : iconSvg(background, accent, unread > 0)
    link.type = 'image/svg+xml'
    link.href = `data:image/svg+xml,${encodeURIComponent(svg)}`
};

/** Colors the tab icon after the active theme. A favicon can't read the page's CSS variables, so
 * the values are read here and the icon is rebuilt as a data URL. This runs in each browser
 * on its own, so two devices on different themes show different icons. */
export const syncFaviconWithTheme = () => draw();

/** How many chats have news. With any, the icon takes turns, like a messenger's tab: the icon with a dot, then the
 * number; with none, it is the plain icon. */
export const setFaviconUnread = (count: number) => {
    if (count === unread) return
    unread = count
    window.clearInterval(timer)
    timer = undefined
    showCount = false
    if (count > 0) {
        timer = window.setInterval(() => {
            showCount = !showCount
            draw()
        }, ALTERNATE_MS)
    }
    draw()
};
