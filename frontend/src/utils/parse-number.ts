/** The number a text field holds, or null when it is empty or isn't one */
export const parseNumber = (text: string): number | null => {
    const trimmed = text.trim()
    if (!trimmed) return null
    const value = Number(trimmed)
    return Number.isFinite(value) ? value : null
};

/** A number for a text field: empty for null */
export const numberText = (value: number | null | undefined): string => (value == null ? '' : String(value))
