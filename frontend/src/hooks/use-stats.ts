import {useEffect, useState} from 'react'
import type {StatsRange} from '../api/stats/types'

/** A stats view over a range, re-fetched when `range` changes. `data` is the last
 * range that loaded and stays put while the next one loads, so switching ranges doesn't blank
 * the page. `load` and `range` must be stable (a
 * module-level API call and one of the page's range constants), since they are what triggers a fetch. */
export const useStats = <T>(load: (range: StatsRange) => Promise<T>, range: StatsRange) => {
    const [loaded, setLoaded] = useState<{ range: StatsRange; data: T } | null>(null)
    const [failedFor, setFailedFor] = useState<StatsRange | null>(null)

    useEffect(() => {
        let cancelled = false
        load(range).then(
            (data) => {
                if (!cancelled) setLoaded({range, data})
            },
            () => {
                if (!cancelled) setFailedFor(range)
            },
        )
        return () => {
            cancelled = true
        }
    }, [load, range])

    return {
        data: loaded?.data ?? null,
        failed: failedFor === range,
    }
};
