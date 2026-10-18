import {ROW_PX} from './scene.ts'
import type {PageLayout} from './types.ts'

/** Rows kept clear at the bottom where no composer covers it, so a scene's floor isn't on the window's very edge */
const BOTTOM_MARGIN_ROWS = 6
/** Rows kept clear above the composer: what stands on the floor (a pot, the cat, the cabin's base) reaches a few rows
 * below the floor line, as tuned in the preview (13 rows clear over a composer about 8 rows tall) */
const ABOVE_COMPOSER_ROWS = 5
/** A centered panel's width when the page has none to measure */
const DEFAULT_CONTENT_PX = 880

/**
 * Reads the layout off the page in front of the background. A page with the sidebar is a chat page; its floor is
 * above the composer when the composer sits at the bottom (a chat), and near the bottom of the window otherwise (the
 * home page, whose composer is in the middle). A page without the sidebar is a centered one.
 */
export function measurePage(): PageLayout {
    const H = window.innerHeight
    const sidebar = document.querySelector('.sidebar')
    if (sidebar) {
        const composer = document.querySelector('.chat .composer')
        const covered = composer ? Math.ceil((H - composer.getBoundingClientRect().top) / ROW_PX) + ABOVE_COMPOSER_ROWS : BOTTOM_MARGIN_ROWS
        return {kind: 'chat', sidebarPx: sidebar.getBoundingClientRect().right, contentPx: DEFAULT_CONTENT_PX, coveredRows: Math.max(BOTTOM_MARGIN_ROWS, covered)}
    }
    const panel = document.querySelector('.page > .frame')
    return {kind: 'center', sidebarPx: 0, contentPx: panel ? panel.getBoundingClientRect().width : DEFAULT_CONTENT_PX, coveredRows: BOTTOM_MARGIN_ROWS}
}

export const sameLayout = (a: PageLayout, b: PageLayout): boolean =>
    a.kind === b.kind && Math.round(a.sidebarPx) === Math.round(b.sidebarPx) && Math.round(a.contentPx) === Math.round(b.contentPx) && a.coveredRows === b.coveredRows
