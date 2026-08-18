import { forwardRef, useImperativeHandle, useLayoutEffect, useRef } from 'react'
import type { CSSProperties, ReactNode, UIEvent } from 'react'

import { Div } from './primitives'

export interface LazyListHandle {
  jumpToTop: () => void
  jumpToBottom: () => void
}

export interface LazyListProps {
  children?: ReactNode
  /** How close to an edge (px) triggers `onTopReached`/`onBottomReached`. */
  threshold?: number
  onTopReached?: () => void
  onBottomReached?: () => void
  style?: CSSProperties
  className?: string
}

/**
 * A scrollable container for incrementally-loaded content (chat history, infinite
 * feeds). Fires `onTopReached`/`onBottomReached` while scrolling within `threshold` of
 * either edge — the caller decides whether/how to load more and guards against firing
 * again while that load is in flight.
 *
 * Whenever `children` changes on its own (e.g. older items prepended after a top-reach
 * load), scroll position is preserved relative to content rather than to the raw
 * scrollbar offset, so growth above the viewport doesn't yank it. `jumpToTop`/
 * `jumpToBottom`, exposed via `ref`, are the explicit override for when the caller
 * wants an edge snap instead (e.g. after the user sends a message).
 */
export const LazyList = forwardRef<LazyListHandle, LazyListProps>(function LazyList(
  { children, threshold = 80, onTopReached, onBottomReached, style, className },
  ref,
) {
  const innerRef = useRef<HTMLDivElement>(null)
  const anchorRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null)

  useImperativeHandle(
    ref,
    () => ({
      jumpToTop: () => {
        requestAnimationFrame(() => {
          if (innerRef.current) innerRef.current.scrollTop = 0
        })
      },
      jumpToBottom: () => {
        requestAnimationFrame(() => {
          if (innerRef.current) innerRef.current.scrollTop = innerRef.current.scrollHeight
        })
      },
    }),
    [],
  )

  // Restore position relative to the last known anchor, then re-anchor to the
  // post-restore state — runs after every commit, cheap no-op when nothing moved.
  useLayoutEffect(() => {
    const el = innerRef.current
    const anchor = anchorRef.current
    if (el && anchor) el.scrollTop = anchor.scrollTop + (el.scrollHeight - anchor.scrollHeight)
  })
  useLayoutEffect(() => {
    const el = innerRef.current
    if (el) anchorRef.current = { scrollHeight: el.scrollHeight, scrollTop: el.scrollTop }
  })

  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget
    anchorRef.current = { scrollHeight: el.scrollHeight, scrollTop: el.scrollTop }
    if (el.scrollTop < threshold) onTopReached?.()
    if (el.scrollHeight - el.scrollTop - el.clientHeight < threshold) onBottomReached?.()
  }

  return (
    <Div ref={innerRef} style={{ overflowY: 'auto', ...style }} className={className} onScroll={onScroll}>
      {children}
    </Div>
  )
})
