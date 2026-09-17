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

/** A scrollable container for incrementally-loaded content, firing callbacks near either edge */
export const LazyList = forwardRef<LazyListHandle, LazyListProps>(function LazyList(
  { children, threshold = 80, onTopReached, onBottomReached, style, className },
  ref,
) {
  const innerRef = useRef<HTMLDivElement>(null)
  const anchorRef = useRef<{ scrollHeight: number; scrollTop: number } | null>(null)
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
