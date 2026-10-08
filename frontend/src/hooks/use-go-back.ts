import {useCallback} from 'react'
import {useNavigate} from 'react-router-dom'

/** Leaves a page the way the browser's Back button would: to the page it was opened from (a chat, Settings, any
 * other), or to `fallback` when the tab opened right on it (a typed address, a link from outside). Stepping back,
 * rather than pushing the fallback, is what keeps two pages that link to each other from making Back go round
 * between them for ever. */
export const useGoBack = (fallback = '/') => {
    const navigate = useNavigate()
    return useCallback(() => {
        // React Router numbers this tab's entries in `history.state.idx`: 0 is the one the tab opened on
        const idx = (window.history.state as { idx?: number } | null)?.idx ?? 0
        if (idx > 0) navigate(-1)
        else navigate(fallback, {replace: true})
    }, [navigate, fallback])
};
