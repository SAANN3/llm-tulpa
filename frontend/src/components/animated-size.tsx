import {useLayoutEffect, useRef, useState, type ReactNode} from 'react'

export interface AnimatedSizeProps {
    children: ReactNode
    className?: string
    /** A selector, resolved inside this box, for the element whose natural height this box follows —
     * by default the first child. It must be taller or shorter because of its content, not because of
     * this box (a scrolling area that fills the box would only ever report the box's own height). */
    measure?: string
    /** Changes when the measured element is a different one (the selector then matches something new) */
    watch?: unknown
}

/** A box that grows and shrinks to what is inside it instead of jumping: it follows the measured
 * element's height and transitions to it from wherever it is now. Its own border or frame, if
 * it sits inside one, moves with it. Content that changes shape (a step, a tab, a list that loads)
 * only has to live inside it. */
export const AnimatedSize = ({children, className, measure, watch}: AnimatedSizeProps) => {
    const ref = useRef<HTMLDivElement>(null)
    // null until measured: the first size is applied without a transition from nothing
    const [height, setHeight] = useState<number | null>(null)

    useLayoutEffect(() => {
        const target = ref.current?.querySelector<HTMLElement>(measure ?? ':scope > *')
        if (!target) return
        const read = () => setHeight(target.offsetHeight)
        read()
        const observer = new ResizeObserver(read)
        observer.observe(target)
        return () => observer.disconnect()
    }, [measure, watch])

    return (
        <div ref={ref} className={`animated-size${className ? ` ${className}` : ''}`}
             style={height == null ? undefined : {height}}>
            {children}
        </div>
    )
};
