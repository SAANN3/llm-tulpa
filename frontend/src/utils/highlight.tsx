import {Children, cloneElement, isValidElement, type ReactNode} from 'react'

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

/** Recursively highlights occurrences of `query` in React children */
export function highlightInNode(child: ReactNode, query?: string | null): ReactNode {
    if (!query || !query.trim()) return child
    if (typeof child === 'string') {
        return highlightText(child, query)
    }
    if (isValidElement(child)) {
        const props = child.props as {children?: ReactNode}
        if (props && props.children) {
            const nextChildren = Children.map(props.children, (c) => highlightInNode(c, query))
            return cloneElement(child, {}, nextChildren)
        }
    }
    return child
}
