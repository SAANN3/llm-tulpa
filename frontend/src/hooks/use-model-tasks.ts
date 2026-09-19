import {useCallback, useEffect, useRef, useState} from 'react'
import {listTasks} from '../api/llm/tasks'
import type {ModelTask} from '../api/llm/types'

const POLL_MS = 1500

/**
 * The backend's model pulls/imports, kept fresh while any of them is still running. Only asks
 * when `enabled` (the endpoint is owner-only). `onFinished` fires once for each task the
 * moment it stops running — a model just became installed, or a failure worth showing.
 */
export const useModelTasks = (enabled: boolean, onFinished: (task: ModelTask) => void) => {
    const [tasks, setTasks] = useState<ModelTask[]>([])
    const running = useRef(new Set<number>())
    const finished = useRef(onFinished)
    finished.current = onFinished

    const refresh = useCallback(async () => {
        if (!enabled) return
        try {
            const latest = await listTasks()
            setTasks(latest)
            for (const task of latest) {
                if (task.state === 'running') running.current.add(task.id)
                else if (running.current.delete(task.id)) finished.current(task)
            }
        } catch {
            // The next tick retries; a picker that can't reach the backend already says so.
        }
    }, [enabled])

    useEffect(() => {
        refresh()
    }, [refresh])

    const anyRunning = tasks.some((t) => t.state === 'running')
    useEffect(() => {
        if (!anyRunning) return
        const id = setInterval(refresh, POLL_MS)
        return () => clearInterval(id)
    }, [anyRunning, refresh])

    return {tasks, refresh}
};
