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
  // Set synchronously the moment `jumpToTop`/`jumpToBottom` is *called*, not once its
  // deferred `requestAnimationFrame` actually runs — that gap is exactly the bug: any
  // render that commits in between (e.g. the one that first paints the newly-loaded
  // content the jump is reacting to) still runs the anchor-restore effect below
  // against the *old* anchor, which can leave `scrollTop` sitting at some
  // computed-from-stale-data value in between the two edges, not just briefly at the
  // wrong edge. Both layout effects below check this first and skip entirely while
  // it's set, so nothing fights the jump until it actually lands and re-anchors.
  const pendingJumpRef = useRef<'top' | 'bottom' | null>(null)

  useImperativeHandle(
    ref,
    () => ({
      jumpToTop: () => {
        pendingJumpRef.current = 'top'
        requestAnimationFrame(() => {
          const el = innerRef.current
          pendingJumpRef.current = null
          if (!el) return
          el.scrollTop = 0
          anchorRef.current = { scrollHeight: el.scrollHeight, scrollTop: 0 }
        })
      },
      jumpToBottom: () => {
        pendingJumpRef.current = 'bottom'
        requestAnimationFrame(() => {
          const el = innerRef.current
          pendingJumpRef.current = null
          if (!el) return
          el.scrollTop = el.scrollHeight
          anchorRef.current = { scrollHeight: el.scrollHeight, scrollTop: el.scrollTop }
        })
      },
    }),
    [],
  )

  // Restore position relative to the last known anchor, then re-anchor to the
  // post-restore state — runs after every commit, cheap no-op when nothing moved.
  useLayoutEffect(() => {
    if (pendingJumpRef.current) return
    const el = innerRef.current
    const anchor = anchorRef.current
    if (el && anchor) el.scrollTop = anchor.scrollTop + (el.scrollHeight - anchor.scrollHeight)
  })
  useLayoutEffect(() => {
    if (pendingJumpRef.current) return
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
