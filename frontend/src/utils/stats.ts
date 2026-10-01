import {formatDurationShort} from './format.ts'

/** Tokens per second from a token count and the milliseconds it took; null when nothing was timed */
export const tokensPerSecond = (tokens: number, ms: number): number | null => (ms > 0 ? tokens / (ms / 1000) : null);

export const formatSpeed = (speed: number | null): string =>
    speed == null ? '–' : `${speed >= 100 ? Math.round(speed) : speed.toFixed(1)} t/s`;

/** A duration given in milliseconds: "420 ms", "3.4s", "1m 25s" */
export const formatMs = (ms: number | null): string => {
    if (ms == null) return '–'
    if (ms < 1000) return `${Math.round(ms)} ms`
    if (ms < 10_000) return `${(ms / 1000).toFixed(1)}s`
    return formatDurationShort(ms / 1000)
};

export const formatPercent = (part: number, whole: number): string =>
    whole > 0 ? `${Math.round((part / whole) * 100)}%` : '–';
