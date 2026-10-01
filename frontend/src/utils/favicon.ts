const LEAF = 'M0,0 C3,-7 9,-5 12,0 C8,1 3,4 0,0 Z'

// The same icon as `public/favicon.svg`, in the given colors
const iconSvg = (background: string, accent: string) =>
    `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32">` +
    `<rect width="32" height="32" rx="7" fill="${background}"/>` +
    `<g fill="${accent}" transform="translate(16,16)">` +
    [0, 90, 180, 270].map((angle) => `<path d="${LEAF}" transform="rotate(${angle})"/>`).join('') +
    `</g></svg>`;

/** Colors the tab icon after the active theme. A favicon can't read the page's CSS variables, so
 * the values are read here and the icon is rebuilt as a data URL. This runs in each browser
 * on its own, so two devices on different themes show different icons. */
export const syncFaviconWithTheme = () => {
    const styles = getComputedStyle(document.documentElement)
    const background = styles.getPropertyValue('--color-secondary').trim()
    const accent = styles.getPropertyValue('--color-tertiary').trim()
    const link = document.querySelector<HTMLLinkElement>('link[rel="icon"]')
    // Keeps the static icon when the theme's colors aren't available yet
    if (!link || !background || !accent) return

    link.type = 'image/svg+xml'
    link.href = `data:image/svg+xml,${encodeURIComponent(iconSvg(background, accent))}`
};
