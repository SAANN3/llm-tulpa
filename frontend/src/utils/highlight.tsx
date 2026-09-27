import {Children, type ReactNode} from 'react'

/** Highlights occurrences of `query` within a plain string using `<mark className="search-match-target">` */
export function highlightText(text: string, query?: string | null): ReactNode {
    if (!query || !query.trim() || !text) return text
    const q = query.trim()
    const lowerText = text.toLowerCase()
    const lowerQ = q.toLowerCase()
    if (!lowerText.includes(lowerQ)) return text

    const parts: ReactNode[] = []
    let lastIndex = 0
    let index = lowerText.indexOf(lowerQ)

    while (index !== -1) {
        if (index > lastIndex) {
            parts.push(text.slice(lastIndex, index))
        }
        const match = text.slice(index, index + q.length)
        parts.push(
            <mark key={index} className="search-match-target">
                {match}
            </mark>,
        )
        lastIndex = index + q.length
        index = lowerText.indexOf(lowerQ, lastIndex)
    }

    if (lastIndex < text.length) {
        parts.push(text.slice(lastIndex))
    }

    return parts
}

/**
 * Highlights occurrences of `query` among a tag's children — a plain string is
 * scanned directly, and an array (any tag with more than one child, e.g. text
 * running alongside an inline `<code>`/`<strong>`) is walked so a match sitting in
 * a bare text sibling isn't skipped just because it isn't the sole child.
 *
 * Deliberately does *not* recurse into a child that's itself a React element (a
 * `<code>`/`<strong>`/etc. produced by another entry in the same `components` map):
 * every such tag is already independently re-rendered through its own wrapped
 * component, which runs this same highlighting on its own children when React
 * actually renders it — recursing into it here too would highlight it a second
 * time, wrapping its already-produced `<mark>` in another `<mark>`.
 */
export function highlightInNode(child: ReactNode, query?: string | null): ReactNode {
    if (!query || !query.trim()) return child
    if (Array.isArray(child)) {
        return Children.map(child, (c) => highlightInNode(c, query))
    }
    if (typeof child === 'string') {
        return highlightText(child, query)
    }
    return child
}
