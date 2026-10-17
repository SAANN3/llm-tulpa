/** The widest window, in pixels, that gets the phone layout: the same number as `$mobile-max` in `styles/_tokens.scss`,
 * which the stylesheets' media queries use. */
export const MOBILE_MAX_WIDTH = 768

export const isMobileViewport = (): boolean => window.innerWidth <= MOBILE_MAX_WIDTH
