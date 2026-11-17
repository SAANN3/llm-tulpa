import {useCallback, useEffect, useRef, useState} from 'react'
import axios from 'axios'
import {answerBlueprintRun} from '../../api/blueprints/answer'
import {cancelQueuedRun} from '../../api/blueprints/cancel-queued'
import {clearBlueprintQueue} from '../../api/blueprints/clear-queue'
import {getBlueprintRun} from '../../api/blueprints/get-run'
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
const ACTIVE = new Set(['queued', 'running', 'waiting_for_permission', 'stopping'])

const errorText = (error: unknown, fallback: string) => errorReason(error, fallback)

export const useBlueprintRuns = (blueprintId: number) => {
    const [queue, setQueue] = useState<QueueState | null>(null)
    const [live, setLive] = useState<RunDetail | null>(null)
    const [names, setNames] = useState<Record<number, string>>({})
    const [nextDue, setNextDue] = useState<string | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [acting, setActing] = useState(false)
    const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
    const refresh = useRef<() => void>(() => {})

    const load = useCallback(async (signal?: AbortSignal) => {
        if (!Number.isSafeInteger(blueprintId) || blueprintId <= 0) return
        const [nextQueue, listed] = await Promise.all([
            getBlueprintQueue(signal),
            listBlueprints({offset: 0, limit: 100, q: '', sort: 'updated'}, signal),
        ])
        if (signal?.aborted) return
        setQueue(nextQueue)
        setNames(Object.fromEntries(listed.items.map(item => [item.id, item.name])))
        setNextDue(listed.items.find(item => item.id === blueprintId)?.next_due_at ?? null)
        const running = nextQueue.running && nextQueue.running.blueprint_id === blueprintId && ACTIVE.has(nextQueue.running.status)
            ? nextQueue.running.id : null
        setLive(running ? await getBlueprintRun(blueprintId, running, signal) : null)
    }, [blueprintId])

    useEffect(() => {
        const controller = new AbortController()
        setError(null)
        if (!Number.isSafeInteger(blueprintId) || blueprintId <= 0) return
        load(controller.signal).catch((reason: unknown) => {
            if (!controller.signal.aborted) setError(errorText(reason, 'Could not load runs.'))
        })
        return () => controller.abort()
    }, [blueprintId, load])

    refresh.current = () => {
        if (timer.current) clearTimeout(timer.current)
        timer.current = setTimeout(() => {
            timer.current = null
            load().catch((reason: unknown) => setError(errorText(reason, 'Could not refresh runs.')))
        }, REFRESH_MS)
    }
    useServerEvent('blueprint', event => {
        const inner = event.event
        if (inner.type === 'queue_changed' || (inner.type === 'run_changed' && inner.blueprint_id === blueprintId)) refresh.current()
    })
    useServerEvent('stream_open', () => refresh.current())

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
        queue, live, names, nextDue, error, acting,
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
