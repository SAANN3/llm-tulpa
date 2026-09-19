import { forwardRef, useImperativeHandle, useLayoutEffect, useRef } from 'react'
import type { CSSProperties, ReactNode, UIEvent } from 'react'

import { Div } from './primitives'

export interface LazyListHandle {
  jumpToTop: () => void
  jumpToBottom: () => void
}

export interface LazyListProps {
  children?: ReactNode
  threshold?: number
  onTopReached?: () => void
  onBottomReached?: () => void
  style?: CSSProperties
  className?: string
}

/**
 * A scrollable container for incrementally-loaded content (chat history). Fires
 * `onTopReached`/`onBottomReached` within `threshold` px of either edge — the caller decides
 * whether to load more and guards against firing again while that load is in flight.
 *
 * When `children` change on their own (older items prepended after a top-reach load), scroll
 * position is kept relative to the content rather than to the raw scrollbar offset, so growth
 * above the viewport doesn't yank it. `jumpToTop`/`jumpToBottom` (via `ref`) are the explicit
 * override for when the caller wants an edge snap instead, e.g. after sending a message.
 */
export const LazyList = forwardRef<LazyListHandle, LazyListProps>(function LazyList(
  { children, threshold = 80, onTopReached, onBottomReached, style, className },
  ref,
) {
  const innerRef = useRef<HTMLDivElement>(null)
  const anchorRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null)
  // Set synchronously when a jump is *called*, not when its deferred `requestAnimationFrame`
  // runs — a render committing in that gap (the one that paints the content the jump is
  // reacting to) would otherwise run the anchor-restore effect below against the old anchor and
  // leave `scrollTop` somewhere between the two edges. Both layout effects skip while it's set.
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

  // After every commit: restore position relative to the last known anchor, then re-anchor to
  // the post-restore state. A cheap no-op when nothing moved.
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
