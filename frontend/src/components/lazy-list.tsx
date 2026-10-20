import { forwardRef, useEffect, useImperativeHandle, useLayoutEffect, useRef } from 'react'
import type { CSSProperties, ReactNode, UIEvent } from 'react'

import { Div } from './primitives'

export interface LazyListHandle {
  jumpToTop: () => void
  jumpToBottom: () => void
  /** Smoothly scrolls the element carrying `data-message-id` into view (centered); true when it was in the DOM */
  scrollToMessage: (messageId: string) => boolean
  /**
   * Runs `mutate` (a state update that resizes some element inside the list, e.g. collapsing an
   * expanded panel) and keeps `anchorEl` pinned at its current position on screen, instead of the
   * list's default "hold distance from the bottom" behavior — which is only correct when growth
   * happens off-screen (prepending older messages, appending a new one below the fold); a resize
   * the viewer is already scrolled into the middle of needs the resized element itself as the
   * fixed point, or the view jumps by the collapsed height instead of staying where you were
   * looking.
   */
  preserveViewportPosition: (anchorEl: HTMLElement, mutate: () => void) => void
}

export interface LazyListProps {
  children?: ReactNode
  threshold?: number
  onTopReached?: () => void
  onBottomReached?: () => void
  style?: CSSProperties
  className?: string
}

/** Where the list was last known to be: what its position is kept relative to when its content or its own height changes */
interface Anchor {
  scrollHeight: number
  scrollTop: number
  clientHeight: number
  /** The first item that shows in the list, and how far below the list's top edge it was */
  item: Element | null
  itemOffset: number
}

const anchorOf = (el: HTMLElement): Anchor => {
  const top = el.getBoundingClientRect().top
  let item: Element | null = null
  for (const child of Array.from(el.children)) {
    if (child.getBoundingClientRect().bottom > top) {
      item = child
      break
    }
  }
  return {
    scrollHeight: el.scrollHeight,
    scrollTop: el.scrollTop,
    clientHeight: el.clientHeight,
    item,
    itemOffset: item ? item.getBoundingClientRect().top - top : 0,
  }
}

/** A list this close to its bottom (px) was at the bottom: scroll rounding and sub-pixel heights leave a pixel or two */
const BOTTOM_TOLERANCE = 2

/**
 * Where `scrollTop` has to be after `anchor` for the position to hold. A list that was at the bottom stays there,
 * whether the content grew or the list itself got shorter (something under it grew). One that was scrolled up keeps the
 * first item it showed where it was: older items added above push it down and the list scrolls with them, a reply
 * growing below it moves nothing. (The distance from the bottom can't stand in for that: it holds only when the
 * growth is above, and drags a list that is being read down with every piece of a reply.)
 */
const keepPosition = (el: HTMLElement, anchor: Anchor): number => {
  if (anchor.scrollHeight - anchor.scrollTop - anchor.clientHeight <= BOTTOM_TOLERANCE) return el.scrollHeight - el.clientHeight
  if (anchor.item && el.contains(anchor.item)) {
    return el.scrollTop + (anchor.item.getBoundingClientRect().top - el.getBoundingClientRect().top - anchor.itemOffset)
  }
  // The item is gone (it was removed): the distance from the bottom is the best that is left
  return anchor.scrollTop + (el.scrollHeight - anchor.scrollHeight)
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
  const anchorRef = useRef<Anchor | null>(null)
  // Set synchronously when a jump is *called*, not when its deferred `requestAnimationFrame`
  // runs — a render committing in that gap (the one that paints the content the jump is
  // reacting to) would otherwise run the anchor-restore effect below against the old anchor and
  // leave `scrollTop` somewhere between the two edges. Both layout effects skip while it's set.
  const pendingJumpRef = useRef<'top' | 'bottom' | 'message' | 'external' | null>(null)

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
          anchorRef.current = anchorOf(el)
        })
      },
      jumpToBottom: () => {
        pendingJumpRef.current = 'bottom'
        requestAnimationFrame(() => {
          const el = innerRef.current
          pendingJumpRef.current = null
          if (!el) return
          el.scrollTop = el.scrollHeight
          anchorRef.current = anchorOf(el)
        })
      },
      scrollToMessage: (messageId: string): boolean => {
        const el = innerRef.current
        if (!el) return false
        const target = el.querySelector(`[data-message-id="${CSS.escape(messageId)}"]`)
        if (!target) return false
        pendingJumpRef.current = 'message'
        requestAnimationFrame(() => {
          pendingJumpRef.current = null
          const matchEl = target.querySelector('.search-match-target')
          ;(matchEl ?? target).scrollIntoView({ behavior: 'smooth', block: 'center' })
        })
        return true
      },
      preserveViewportPosition: (anchorEl, mutate) => {
        pendingJumpRef.current = 'external'
        const before = anchorEl.getBoundingClientRect().top
        mutate()
        // `mutate` triggers a state update; by the time this callback runs, React has
        // already committed and the browser has laid the new DOM out (rAF only fires
        // after that), so `anchorEl`'s new position is real, not stale.
        requestAnimationFrame(() => {
          const el = innerRef.current
          pendingJumpRef.current = null
          if (!el) return
          el.scrollTop += anchorEl.getBoundingClientRect().top - before
          anchorRef.current = anchorOf(el)
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
    if (el && anchor) el.scrollTop = keepPosition(el, anchor)
  })
  useLayoutEffect(() => {
    if (pendingJumpRef.current) return
    const el = innerRef.current
    if (el) anchorRef.current = anchorOf(el)
  })

  // The list gets shorter or taller without its content changing when what sits under it does (the run's status block
  // grows with its graph and its steps): a commit of this component isn't what tells, so the size is watched. Held
  // at the bottom it stays there, so the last lines are never left under the block.
  useEffect(() => {
    const el = innerRef.current
    if (!el) return
    const watch = new ResizeObserver(() => {
      const anchor = anchorRef.current
      if (pendingJumpRef.current || !anchor) return
      el.scrollTop = keepPosition(el, anchor)
      anchorRef.current = anchorOf(el)
    })
    watch.observe(el)
    return () => watch.disconnect()
  }, [])

  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget
    anchorRef.current = anchorOf(el)
    if (el.scrollTop < threshold) onTopReached?.()
    if (el.scrollHeight - el.scrollTop - el.clientHeight < threshold) onBottomReached?.()
  }

  return (
    <Div ref={innerRef} style={{ overflowY: 'auto', ...style }} className={className} onScroll={onScroll}>
      {children}
    </Div>
  )
})
