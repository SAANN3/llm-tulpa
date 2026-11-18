import {useCallback, useEffect, useRef, useState} from 'react'
import axios from 'axios'
import {answerBlueprintRun} from '../../api/blueprints/answer'
import {cancelQueuedRun} from '../../api/blueprints/cancel-queued'
import {clearBlueprintQueue} from '../../api/blueprints/clear-queue'
import {getBlueprintRun} from '../../api/blueprints/get-run'
import {listBlueprintRuns} from '../../api/blueprints/history'
import {listBlueprints} from '../../api/blueprints/list'
import {getBlueprintQueue} from '../../api/blueprints/queue'
import {reorderBlueprintQueue} from '../../api/blueprints/reorder'
import {startBlueprint} from '../../api/blueprints/start'
import {stopBlueprintRun} from '../../api/blueprints/stop'
import type {QueueState, RunDetail} from '../../api/blueprints/types'
import type {Allowance} from '../../api/agent/types'
import {useServerEvent} from '../use-server-events'
import {errorReason} from '../../utils/error-reason'

const REFRESH_MS = 250
const TRACK_MS = 400
const ACTIVE = new Set(['queued', 'running', 'waiting_for_permission', 'stopping'])

const errorText = (error: unknown, fallback: string) => errorReason(error, fallback)

export const useBlueprintRuns = (blueprintId: number) => {
    const [queue, setQueue] = useState<QueueState | null>(null)
    const [detail, setDetail] = useState<RunDetail | null>(null)
    const finished = useRef<RunDetail | null>(null)
    const requestId = useRef(0)
    const [names, setNames] = useState<Record<number, string>>({})
    const [nextDue, setNextDue] = useState<string | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [acting, setActing] = useState(false)
    const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
    const refreshPending = useRef(false)
    const queueRefreshPending = useRef(false)
    const refreshController = useRef<AbortController | null>(null)
    const refresh = useRef<(includeQueue?: boolean) => void>(() => {})
    const cancelRefresh = useCallback(() => {
        requestId.current++
        if (timer.current !== null) clearTimeout(timer.current)
        timer.current = null
        refreshPending.current = false
        queueRefreshPending.current = false
        refreshController.current?.abort()
        refreshController.current = null
    }, [])

    const load = useCallback(async (signal?: AbortSignal) => {
        if (!Number.isSafeInteger(blueprintId) || blueprintId <= 0) return
        const request = ++requestId.current
        const [nextQueue, listed] = await Promise.all([
            getBlueprintQueue(signal),
            listBlueprints({offset: 0, limit: 100, q: '', sort: 'updated'}, signal),
        ])
        if (signal?.aborted || request !== requestId.current) return
        setQueue(nextQueue)
        setNames(Object.fromEntries(listed.items.map(item => [item.id, item.name])))
        setNextDue(listed.items.find(item => item.id === blueprintId)?.next_due_at ?? null)
        const running = nextQueue.running?.blueprint_id === blueprintId ? nextQueue.running : null
        const summary = running ?? (await listBlueprintRuns(blueprintId, {offset: 0, limit: 1}, signal)).items[0]
        if (signal?.aborted || request !== requestId.current) return
        if (!summary) {
            setDetail(null)
            return
        }
        const cached = finished.current
        const next = cached?.run.id === summary.id ? cached : await getBlueprintRun(blueprintId, summary.id, signal)
        if (signal?.aborted || request !== requestId.current) return
        if (!ACTIVE.has(next.run.status)) finished.current = next
        setDetail(next)
    }, [blueprintId])

    useEffect(() => {
        const controller = new AbortController()
        setError(null)
        if (!Number.isSafeInteger(blueprintId) || blueprintId <= 0) return
        load(controller.signal).catch((reason: unknown) => {
            if (!controller.signal.aborted) setError(errorText(reason, 'Could not load runs.'))
        })
        return () => {
            controller.abort()
            cancelRefresh()
        }
    }, [blueprintId, load, cancelRefresh])

    const result = detail?.run.blueprint_id === blueprintId ? detail : null
    const live = result && queue?.running?.id === result.run.id && ACTIVE.has(result.run.status) ? result : null
    const activeId = result && ACTIVE.has(result.run.status) ? result.run.id : null

    refresh.current = (includeQueue = false) => {
        refreshPending.current = true
        queueRefreshPending.current ||= includeQueue
        if (timer.current !== null || refreshController.current) return
        // Node events can arrive continuously in a loop. Read at a fixed cadence while
        // they arrive, then catch up once if state changed during the request.
        timer.current = setTimeout(async () => {
            timer.current = null
            refreshPending.current = false
            const includeQueue = queueRefreshPending.current
            queueRefreshPending.current = false
            const controller = new AbortController()
            refreshController.current = controller
            try {
                if (includeQueue || !live) {
                    await load(controller.signal)
                } else {
                    const request = ++requestId.current
                    const next = await getBlueprintRun(blueprintId, live.run.id, controller.signal)
                    if (controller.signal.aborted || request !== requestId.current) return
                    if (!ACTIVE.has(next.run.status)) finished.current = next
                    setDetail(next)
                    if (next.run.status !== live.run.status) {
                        await load(controller.signal)
                    } else {
                        setQueue(current => current?.running?.id === next.run.id ? {...current, running: next.run} : current)
                    }
                }
            } catch (reason) {
                if (!controller.signal.aborted) setError(errorText(reason, 'Could not refresh runs.'))
            } finally {
                if (!controller.signal.aborted) {
                    refreshController.current = null
                    if (refreshPending.current) refresh.current()
                }
            }
        }, REFRESH_MS)
    }
    useServerEvent('blueprint', ({event}) => {
        if (event.type === 'queue_changed') refresh.current(true)
        else if (event.blueprint_id === blueprintId) refresh.current(event.run_id !== live?.run.id)
    })
    useServerEvent('stream_open', () => refresh.current(true))

    useEffect(() => {
        if (activeId === null) return
        // The stream may reconnect late or a proxy may buffer it. Keep a known run
        // visible through its last snapshot even if there are no events to wake us.
        const tick = window.setInterval(() => {
            if (!document.hidden) refresh.current()
        }, TRACK_MS)
        return () => window.clearInterval(tick)
    }, [activeId])

    const act = async (work: () => Promise<void>, fallback: string) => {
        setActing(true)
        setError(null)
        try {
            await work()
        } catch (reason) {
            setError(errorText(reason, fallback))
            if (axios.isAxiosError(reason) && reason.response?.status === 409) {
                load().catch(() => {})
            }
        } finally {
            setActing(false)
        }
    }

    return {
        queue, live, result, names, nextDue, error, acting,
        start: (revision: number) => act(async () => {
            try {
                await startBlueprint(blueprintId, revision)
            } catch (reason) {
                if (axios.isAxiosError(reason) && reason.response?.status === 409) {
                    setError('The saved blueprint changed. These edits stay here; reload it before running.')
                    await load()
                    return
                }
                throw reason
            }
            await load()
        }, 'Could not start the blueprint.'),
        stop: () => act(async () => {
            await stopBlueprintRun()
            await load()
        }, 'Could not stop the run.'),
        move: (index: number, delta: number) => act(async () => {
            const ids = (queue?.queued ?? []).map(item => item.id)
            const next = index + delta
            if (next < 0 || next >= ids.length) return
            const [item] = ids.splice(index, 1)
            ids.splice(next, 0, item)
            try {
                setQueue(await reorderBlueprintQueue(ids))
            } catch (reason) {
                if (axios.isAxiosError(reason) && reason.response?.status === 409) setQueue(await getBlueprintQueue())
                throw reason
            }
        }, 'Could not reorder the queue.'),
        clear: () => act(async () => {
            await clearBlueprintQueue()
            await load()
        }, 'Could not clear the queue.'),
        remove: (runId: number) => act(async () => {
            await cancelQueuedRun(runId)
            await load()
        }, 'Could not remove that run from the queue.'),
        answer: (allowance: Allowance) => act(async () => {
            const pending = live?.state.pending_tool
            if (!pending || !live) return
            await answerBlueprintRun(blueprintId, live.run.id, pending.request_id, allowance)
            await load()
        }, 'Could not answer the permission request.'),
    }
}
