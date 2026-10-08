import {Link as RouterLink} from 'react-router-dom'
import type {LinkProps, ThemedProps} from './types'

/** An in-app link: a click moves within the page without reloading it, while a middle click, Ctrl+click or a long
 * press opens it in a new tab like any link. `variant` is left off for a plain row; give one for a link that looks
 * like a button (with the `link-button` class). */
export const Link = ({to, style, className, variant, title, children, onContextMenu}: ThemedProps<LinkProps>) => (
    <RouterLink to={to} style={style} className={className} data-variant={variant} title={title} onContextMenu={onContextMenu}>
        {children}
    </RouterLink>
);
