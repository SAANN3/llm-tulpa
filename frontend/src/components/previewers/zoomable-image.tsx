import {useLayoutEffect, useRef, useState} from 'react'
import {Button} from '../primitives'
import {useWindowTools} from '../popups/base/window-tools.ts'

const STEPS = [0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4]
const MIN_ZOOM = 0.05
const MAX_ZOOM = 8

export interface ZoomableImageProps {
    src: string
    alt: string
}

/** An image fitted to its window, or zoomed: in steps with the buttons and `+`/`-`, smoothly with Ctrl+scroll around
 * the pointer, scrolling when it outgrows the window. The zoom and the image's size go in the window's tool row, with
 * `0` fitting it again */
export const ZoomableImage = ({src, alt}: ZoomableImageProps) => {
    // null: fitted to the window
    const [zoom, setZoom] = useState<number | null>(null)
    const [natural, setNatural] = useState<{ width: number; height: number } | null>(null)
    const imgRef = useRef<HTMLImageElement>(null)
    // The image point that was under the pointer and where the pointer was, to put back together after a wheel zoom
    const anchorRef = useRef<{ imageX: number; imageY: number; x: number; y: number; scroller: HTMLElement } | null>(null)

    /** The scale the image is shown at now, fitted or zoomed */
    const shownScale = (): number => {
        const img = imgRef.current
        if (zoom != null || !img || !natural) return zoom ?? 1
        return Math.min(img.clientWidth / natural.width, img.clientHeight / natural.height)
    }

    const step = (direction: 1 | -1) => setZoom(() => {
        const current = shownScale()
        const next = direction > 0 ? STEPS.find((s) => s > current + 0.001) : [...STEPS].reverse().find((s) => s < current - 0.001)
        return next ?? current
    })

    const zoomAt = (factor: number, at: { x: number; y: number }, scroller: HTMLElement) => {
        const img = imgRef.current
        if (!img || !natural) return
        const current = shownScale()
        const rect = img.getBoundingClientRect()
        const scrollerRect = scroller.getBoundingClientRect()
        // A fitted image is letterboxed inside its element; a zoomed one is the element
        const left = rect.left + (zoom == null ? (rect.width - natural.width * current) / 2 : 0)
        const top = rect.top + (zoom == null ? (rect.height - natural.height * current) / 2 : 0)
        anchorRef.current = {
            imageX: (scrollerRect.left + at.x - left) / current,
            imageY: (scrollerRect.top + at.y - top) / current,
            x: at.x,
            y: at.y,
            scroller,
        }
        setZoom(Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, current * factor)))
    }

    // After a wheel zoom: scroll so the image point that was under the pointer is under it again
    useLayoutEffect(() => {
        const img = imgRef.current
        const anchor = anchorRef.current
        if (!img || !anchor || zoom == null) return
        anchorRef.current = null
        anchor.scroller.scrollLeft = img.offsetLeft + anchor.imageX * zoom - anchor.x
        anchor.scroller.scrollTop = img.offsetTop + anchor.imageY * zoom - anchor.y
    }, [zoom])

    useWindowTools({
        node: (
            <>
                <Button variant="secondary" className="preview-tool" text="−" title="Zoom out (-)" onClicked={() => step(-1)}/>
                <span>{zoom == null ? 'Fit' : `${Math.round(zoom * 100)}%`}</span>
                <Button variant="secondary" className="preview-tool" text="+" title="Zoom in (+)" onClicked={() => step(1)}/>
                <Button variant="secondary" className={`preview-tool${zoom == null ? ' preview-tool--on' : ''}`} text="Fit" title="Fit (0)"
                        onClicked={() => setZoom(null)}/>
            </>
        ),
        meta: natural ? `${natural.width}×${natural.height}` : undefined,
        actions: [
            // One entry on the key line for both directions
            {keys: ['+', '=', '-'], shown: '+ −', label: 'zoom', run: (key) => step(key === '-' ? -1 : 1)},
            {keys: ['0'], label: 'fit', run: () => setZoom(null)},
        ],
        onZoom: zoomAt,
    }, [zoom, natural])

    return (
        <img
            ref={imgRef}
            src={src}
            alt={alt}
            onLoad={(e) => setNatural({width: e.currentTarget.naturalWidth, height: e.currentTarget.naturalHeight})}
            style={zoom == null
                ? {width: '100%', height: '100%', objectFit: 'contain', display: 'block'}
                : {width: natural ? natural.width * zoom : undefined, maxWidth: 'none', display: 'block', margin: '0 auto'}}
        />
    )
};
