/**
 * Trims the `before` context of a search hit so that `matched` is guaranteed
 * to be visible within a single-line ellipsis container across PC and mobile screens.
 */
export function getVisibleBefore(
    before: string | null | undefined,
    matched: string,
    containerCharBudget = 34,
): string | null {
    if (!before) return null

    // We budget the before-context so that `matched` is guaranteed to be visible
    // within a single-line ellipsis container across PC and mobile screens.
    const budget = Math.max(0, Math.min(15, containerCharBudget - matched.length))
    if (budget === 0) return null

    if (before.length <= budget) {
        return before
    }

    const slice = before.slice(-budget)
    // If the slice cuts through a word and there's a space within the first 6 characters, trim to word boundary
    const spaceIndex = slice.indexOf(' ')
    if (spaceIndex > 0 && spaceIndex <= 6) {
        const wordAligned = slice.slice(spaceIndex + 1).trimStart()
        if (wordAligned.length >= 3) {
            return wordAligned
        }
    }

    return slice.trimStart()
}
