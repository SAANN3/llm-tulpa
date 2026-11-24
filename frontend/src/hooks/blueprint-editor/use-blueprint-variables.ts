import {useCallback, useEffect, useRef, useState} from 'react'
import {getBlueprintVariables} from '../../api/blueprints/variables'
import {resetBlueprintVariables} from '../../api/blueprints/reset-variables'
import type {Literal} from '../../api/blueprints/types'
import {errorReason} from '../../utils/error-reason'
import {useServerEvent} from '../use-server-events'

export const useBlueprintVariables = (id: number, revision: number) => {
    const [values, setValues] = useState<Record<string, Literal> | null>(null)
    const [loading, setLoading] = useState(true)
    const [readError, setReadError] = useState<string | null>(null)
    const [resetError, setResetError] = useState<string | null>(null)
    const [resetting, setResetting] = useState(false)
    const readRequest = useRef<AbortController | null>(null)
    const resetRequest = useRef<AbortController | null>(null)

    const reload = useCallback(async () => {
        if (resetRequest.current) return
        readRequest.current?.abort()
        const request = new AbortController()
        readRequest.current = request
        setLoading(true)
        setReadError(null)
        try {
            const next = await getBlueprintVariables(id, request.signal)
            if (!request.signal.aborted) setValues(next)
        } catch (reason) {
            if (!request.signal.aborted) setReadError(errorReason(reason, 'Could not load saved variables.'))
        } finally {
            if (!request.signal.aborted) {
                readRequest.current = null
                setLoading(false)
            }
        }
    }, [id])

    useEffect(() => {
        void reload()
        return () => readRequest.current?.abort()
    }, [reload, revision])

    useEffect(() => () => resetRequest.current?.abort(), [])

    useServerEvent('stream_open', () => { void reload() })
    useServerEvent('blueprint', ({event}) => {
        if (event.type !== 'run_changed' || event.blueprint_id !== id) return
        if (event.status === 'completed' || event.status === 'failed' || event.status === 'cancelled' || event.status === 'interrupted') {
            void reload()
        }
    })

    const reset = async (nodeId?: string) => {
        if (resetRequest.current) return
        readRequest.current?.abort()
        const request = new AbortController()
        resetRequest.current = request
        setResetting(true)
        setResetError(null)
        try {
            await resetBlueprintVariables(id, revision, nodeId, request.signal)
        } catch (reason) {
            if (!request.signal.aborted) setResetError(errorReason(reason, 'Could not reset saved variables.'))
        } finally {
            if (!request.signal.aborted) {
                resetRequest.current = null
                setResetting(false)
                void reload()
            }
        }
    }

    return {values, loading, readError, resetError, resetting, reload, reset}
}
