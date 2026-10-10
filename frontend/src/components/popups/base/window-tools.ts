import {createContext, useContext, useEffect} from 'react'
import type {ReactNode} from 'react'
import type {FrameAction} from '../../frame.tsx'

/** What a window's content adds to the window: controls in its tool row, a note at the row's end (a size, a count),
 * and keys of its own on the window's key line */
export interface WindowTools {
    node?: ReactNode
    meta?: string
    actions?: FrameAction[]
    /** Ctrl+scroll over the window scales the content around the pointer (text, code, a table); the page doesn't zoom */
    scalable?: boolean
    /** Ctrl+scroll is the content's own: it zooms by `factor` keeping the point at `at` (in `scroller`'s visible area)
     * under the pointer. For content with a zoom of its own, an image */
    onZoom?: (factor: number, at: { x: number; y: number }, scroller: HTMLElement) => void
}

export const WindowToolsContext = createContext<{ setTools: (tools: WindowTools | null) => void } | null>(null)

/**
 * Puts `tools` in the window this is shown in, and takes them out when it goes; outside a window it does nothing.
 * `deps` are what the tools are drawn from: they are handed over again only when one of those changes, so a content
 * that re-renders often doesn't re-render the window each time.
 */
export const useWindowTools = (tools: WindowTools, deps: unknown[]) => {
    const window = useContext(WindowToolsContext)
    useEffect(() => {
        window?.setTools(tools)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [window, ...deps])
    useEffect(() => () => window?.setTools(null), [window])
}
