import {useEffect, useState} from 'react'
import {getUsageStats} from '../api/llm/stats'
import type {UsageStats} from '../api/llm/types'

/** The user's token usage over the last `days` days; re-fetches when `days` changes. `stats` is
 * the last range that loaded, with `loadedDays` saying which — it stays put while the next
 * range loads, so switching ranges doesn't blank the page. */
export const useUsageStats = (days: number) => {
    const [loaded, setLoaded] = useState<{ days: number; stats: UsageStats } | null>(null)
    const [failedFor, setFailedFor] = useState<number | null>(null)

    useEffect(() => {
        let cancelled = false
        getUsageStats(days).then(
            (stats) => {
                if (!cancelled) setLoaded({days, stats})
            },
            () => {
                if (!cancelled) setFailedFor(days)
            },
        )
        return () => {
            cancelled = true
        }
    }, [days])

    return {
        stats: loaded?.stats ?? null,
        loadedDays: loaded?.days ?? null,
        failed: failedFor === days,
    }
};
