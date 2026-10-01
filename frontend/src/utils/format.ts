/** A token count in compact form: 980 stays as-is, 3400 → "3.4k", 15000 → "15k", 131072 → "131k",
 * 22856262 → "22.9M" */
export const formatTokenCount = (tokens: number): string => {
    if (tokens < 1000) return `${tokens}`
    if (tokens >= 1_000_000) return `${Math.round(tokens / 100_000) / 10}M`
    const k = tokens / 1000
    return `${k >= 100 ? Math.round(k) : Math.round(k * 10) / 10}k`
}

/** A duration in "12s" or "1m 25s" form — seconds while under a minute, then minutes and seconds */
export const formatDurationShort = (totalSeconds: number): string => {
    if (totalSeconds < 60) return `${Math.floor(totalSeconds)}s`
    const minutes = Math.floor(totalSeconds / 60)
    const seconds = Math.floor(totalSeconds) % 60
    return `${minutes}m ${seconds}s`
}
