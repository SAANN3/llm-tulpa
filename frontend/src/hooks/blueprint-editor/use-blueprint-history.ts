import {useCallback, useEffect, useRef, useState} from 'react'
import axios from 'axios'
import {getBlueprintRun} from '../../api/blueprints/get-run'
import {listBlueprintRuns} from '../../api/blueprints/history'
import type {RunDetail, RunSummary} from '../../api/blueprints/types'
import {useServerEvent} from '../use-server-events'
import {errorReason} from '../../utils/error-reason'

const PAGE = 20
const REFRESH_MS = 250

export const useBlueprintHistory = (blueprintId: number, runId: number | null, updateList: (update: () => void) => void) => {
    const [items, setItems] = useState<RunSummary[]>([])
    const [total, setTotal] = useState(0)
    const [detail, setDetail] = useState<RunDetail | null>(null)
    const [detailError, setDetailError] = useState<string | null>(null)
    const detailRequest = useRef<AbortController | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [loading, setLoading] = useState(true)
    const [loadingMore, setLoadingMore] = useState(false)
    const shown = useRef(PAGE)
    const moreRequest = useRef<AbortController | null>(null)
    const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
    const refresh = useRef<() => void>(() => {})

    const loadList = useCallback(async (signal?: AbortSignal) => {
        if (!Number.isSafeInteger(blueprintId) || blueprintId <= 0) return
        const count = shown.current
        const next: RunSummary[] = []
        let total = 0
        do {
            const page = await listBlueprintRuns(blueprintId, {offset: next.length, limit: Math.min(PAGE, count - next.length)}, signal)
            if (signal?.aborted) return
            next.push(...page.items)
            total = page.total
            if (!page.items.length) break
        } while (next.length < count && next.length < total)
        updateList(() => {
            setItems(next)
            setTotal(total)
        })
    }, [blueprintId, updateList])

    const loadDetail = useCallback(async () => {
        detailRequest.current?.abort()
        if (runId === null) return
        const controller = new AbortController()
        detailRequest.current = controller
        try {
            const next = await getBlueprintRun(blueprintId, runId, controller.signal)
            if (!controller.signal.aborted) {
                setDetail(next)
                setDetailError(null)
            }
        } catch (reason) {
            if (controller.signal.aborted) return
            if (axios.isAxiosError(reason) && reason.response?.status === 404) setDetail(null)
            setDetailError(errorReason(reason, 'Could not open that run.'))
        }
    }, [blueprintId, runId])

    useEffect(() => {
        const controller = new AbortController()
        shown.current = PAGE
        moreRequest.current?.abort()
        moreRequest.current = null
        setItems([])
        setTotal(0)
        setLoadingMore(false)
        setError(null)
        setDetail(null)
        setLoading(true)
        if (!Number.isSafeInteger(blueprintId) || blueprintId <= 0) {
            setError('Invalid blueprint id.')
            setLoading(false)
            return
        }
        loadList(controller.signal)
            .catch((reason: unknown) => {
                if (!controller.signal.aborted) setError(errorReason(reason, 'Could not load run history.'))
            })
            .finally(() => { if (!controller.signal.aborted) setLoading(false) })
        return () => {
            controller.abort()
            moreRequest.current?.abort()
            if (timer.current) clearTimeout(timer.current)
        }
    }, [blueprintId, loadList])

    useEffect(() => {
        setDetail(null)
        setDetailError(null)
        void loadDetail()
        return () => detailRequest.current?.abort()
    }, [loadDetail])

    refresh.current = () => {
        if (timer.current) clearTimeout(timer.current)
        timer.current = setTimeout(() => {
            timer.current = null
            Promise.all([loadList(), loadDetail()]).catch((reason: unknown) => setError(errorReason(reason, 'Could not refresh run history.')))
        }, REFRESH_MS)
    }
    useServerEvent('blueprint', event => {
        const inner = event.event
        if (inner.type === 'queue_changed' || (inner.type === 'run_changed' && inner.blueprint_id === blueprintId)) refresh.current()
    })
    useServerEvent('stream_open', () => refresh.current())

    const loadMore = async () => {
        if (loading || moreRequest.current || error || items.length >= total) return
        const controller = new AbortController()
        moreRequest.current = controller
        setLoadingMore(true)
        setError(null)
        try {
            const page = await listBlueprintRuns(blueprintId, {offset: items.length, limit: PAGE}, controller.signal)
            if (controller.signal.aborted) return
            shown.current = items.length + page.items.length
            updateList(() => {
                setItems(current => {
                    const ids = new Set(current.map(item => item.id))
                    return [...current, ...page.items.filter(item => !ids.has(item.id))]
                })
                setTotal(page.total)
            })
        } catch (reason) {
            if (!controller.signal.aborted) setError(errorReason(reason, 'Could not load more runs.'))
        } finally {
            if (!controller.signal.aborted) {
                moreRequest.current = null
                setLoadingMore(false)
            }
        }
    }

    return {items, total, detail, detailError, error, loading, loadingMore, loadMore}
}
