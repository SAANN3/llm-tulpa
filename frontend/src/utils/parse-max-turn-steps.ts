/** Parses the step-limit text: empty or 0 is no limit */
export const parseMaxTurnSteps = (text: string): { valid: boolean; value: number } => {
    const trimmed = text.trim()
    if (!trimmed) return {valid: true, value: 0}
    const n = Number(trimmed)
    return {valid: Number.isInteger(n) && n >= 0 && n <= 10_000, value: n}
}
