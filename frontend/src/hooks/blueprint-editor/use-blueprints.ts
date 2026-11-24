import {useCallback, useEffect, useRef, useState} from 'react'
import axios from 'axios'
import {listBlueprints} from '../../api/blueprints/list'
import type {BlueprintList} from '../../api/blueprints/types'

const BLUEPRINT_BATCH_SIZE = 20

export const useBlueprints = (query: string, updateList: (update: () => void, reset: boolean) => void) => {
    const [result, setResult] = useState<BlueprintList | null>(null)
    const [loading, setLoading] = useState(true)
    const [error, setError] = useState<string | null>(null)
    const [refreshId, setRefreshId] = useState(0)
    const [busy, setBusy] = useState(false)
    const mutating = useRef(false)
    const request = useRef<AbortController | null>(null)
    const current = useRef<BlueprintList | null>(null)
    const nextOffset = useRef(0)
    const loadFailed = useRef(false)
    const refresh = useCallback(() => setRefreshId((id) => id + 1), [])

    const load = useCallback((offset: number) => {
        const controller = new AbortController()
        request.current = controller
        loadFailed.current = false
        setLoading(true)
        if (offset === 0) {
            current.current = null
        }
        listBlueprints({q: query, sort: 'updated', offset, limit: BLUEPRINT_BATCH_SIZE}, controller.signal)
            .then((data) => {
                if (controller.signal.aborted) return
                const previous = current.current?.items ?? []
                const ids = new Set(previous.map((item) => item.id))
                const combined = {...data, items: [...previous, ...data.items.filter((item) => !ids.has(item.id))]}
                nextOffset.current = data.offset + data.items.length
                current.current = combined
                updateList(() => setResult(combined), offset === 0)
            })
            .catch((e: unknown) => {
                if (!controller.signal.aborted) {
                    loadFailed.current = true
                    setError(axios.isAxiosError(e)
                        ? e.response?.data?.error ?? 'Could not load blueprints.'
                        : 'Could not load blueprints.')
                }
            })
            .finally(() => {
                if (!controller.signal.aborted) {
                    request.current = null
                    setLoading(false)
                }
            })
    }, [query, updateList])

    useEffect(() => {
        load(0)
        return () => request.current?.abort()
    }, [load, refreshId])

    const loadOlder = () => {
        if (request.current || loadFailed.current || error || !current.current || nextOffset.current >= current.current.total) return
        load(nextOffset.current)
    }

    const mutate = async (action: () => Promise<unknown>): Promise<boolean> => {
        if (mutating.current) return false
        mutating.current = true
        setBusy(true)
        setError(null)
        try {
            await action()
            refresh()
            return true
        } catch (e) {
            if (axios.isAxiosError(e) && (e.response?.status === 409 || e.response?.status === 404)) {
                setError('This blueprint changed or was removed. The list has been reloaded; review it before trying again.')
                refresh()
            } else {
                setError(axios.isAxiosError(e) ? e.response?.data?.error ?? 'Could not save the change.' : 'Could not save the change.')
            }
            return false
        } finally {
            mutating.current = false
            setBusy(false)
        }
    }

    return {result, loading, error, busy, mutate, refresh, loadOlder, clearError: () => setError(null)}
};
