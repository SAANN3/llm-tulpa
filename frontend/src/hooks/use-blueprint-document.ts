import {useCallback, useEffect, useRef, useState} from 'react'
import axios from 'axios'
import {getBlueprint} from '../api/blueprints/get'
import {getBlueprintCatalog} from '../api/blueprints/catalog'
import {updateBlueprint} from '../api/blueprints/update'
import {validateBlueprint} from '../api/blueprints/validate'
import type {BlueprintDocument, NodeCatalog, ValidationReport, Schedule, Workflow} from '../api/blueprints/types'

const errorText = (error: unknown, fallback: string): string =>
    axios.isAxiosError(error) ? error.response?.data?.error ?? fallback : fallback

const HISTORY_LIMIT = 100

type Content = {name: string; schedule: Schedule; workflow: Workflow}
const content = (document: BlueprintDocument): Content => ({name: document.blueprint.name, schedule: document.blueprint.schedule, workflow: document.workflow})

export const useBlueprintDocument = (id: number) => {
    const [draft, setDraft] = useState<BlueprintDocument | null>(null)
    const [baseline, setBaseline] = useState<BlueprintDocument | null>(null)
    const [catalog, setCatalog] = useState<NodeCatalog | null>(null)
    const [loading, setLoading] = useState(true)
    const [busy, setBusy] = useState<'saving' | 'validating' | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [conflict, setConflict] = useState(false)
    const [report, setReport] = useState<ValidationReport | null>(null)
    const [refreshId, setRefreshId] = useState(0)
    const history = useRef<{past: Content[]; future: Content[]}>({past: [], future: []})
    const current = useRef<BlueprintDocument | null>(null)
    const operation = useRef<AbortController | null>(null)
    const reload = useCallback(() => setRefreshId(value => value + 1), [])

    useEffect(() => {
        const controller = new AbortController()
        operation.current?.abort()
        operation.current = null
        current.current = null
        history.current = {past: [], future: []}
        setDraft(null)
        setLoading(true)
        setError(null)
        setConflict(false)
        setReport(null)
        setBusy(null)
        if (!Number.isSafeInteger(id) || id <= 0) {
            setError('Invalid blueprint id.')
            setLoading(false)
            return
        }
        Promise.all([getBlueprint(id, controller.signal), getBlueprintCatalog(controller.signal)])
            .then(([document, definitions]) => {
                if (controller.signal.aborted) return
                current.current = document
                setDraft(document)
                setBaseline(document)
                setCatalog(definitions)
            })
            .catch((error: unknown) => {
                if (!controller.signal.aborted) setError(errorText(error, 'Could not load the blueprint.'))
            })
            .finally(() => { if (!controller.signal.aborted) setLoading(false) })
        return () => { controller.abort(); operation.current?.abort() }
    }, [id, refreshId])

    const edit = (update: (value: BlueprintDocument) => BlueprintDocument) => {
        if (!current.current || operation.current) return
        const next = update(current.current)
        history.current.past.push(content(current.current))
        history.current.past = history.current.past.slice(-HISTORY_LIMIT)
        history.current.future = []
        current.current = next
        setDraft(next)
        setReport(null)
    }
    const travel = (direction: 'undo' | 'redo') => {
        if (!current.current || operation.current) return
        const source = direction === 'undo' ? history.current.past : history.current.future
        const target = direction === 'undo' ? history.current.future : history.current.past
        const snapshot = source.pop()
        if (!snapshot) return
        target.push(content(current.current))
        // Undo edits the content, keeping the latest server revision for the next save.
        const next = {...current.current, blueprint: {...current.current.blueprint, name: snapshot.name, schedule: snapshot.schedule}, workflow: snapshot.workflow}
        current.current = next
        setDraft(next)
        setReport(null)
    }
    const save = async () => {
        const snapshot = current.current
        if (!snapshot || operation.current || conflict) return false
        const controller = new AbortController()
        operation.current = controller
        setBusy('saving')
        setError(null)
        try {
            const blueprint = await updateBlueprint(snapshot.blueprint, snapshot.blueprint.name, snapshot.workflow, controller.signal)
            if (controller.signal.aborted) return false
            const saved = {...snapshot, blueprint}
            current.current = saved
            setDraft(saved)
            setBaseline(saved)
            return true
        } catch (error) {
            if (controller.signal.aborted) return false
            if (axios.isAxiosError(error) && error.response?.status === 409) {
                setConflict(true)
                setError('This blueprint changed elsewhere. Your edits are kept here; reload to review the latest version.')
            } else setError(errorText(error, 'Could not save the blueprint.'))
            return false
        } finally {
            if (!controller.signal.aborted) { operation.current = null; setBusy(null) }
        }
    }
    const validate = async () => {
        if (!current.current || operation.current) return
        const controller = new AbortController()
        operation.current = controller
        setBusy('validating')
        setError(null)
        try {
            const result = await validateBlueprint(current.current.workflow, controller.signal)
            if (!controller.signal.aborted) setReport(result)
        } catch (error) {
            if (!controller.signal.aborted) setError(errorText(error, 'Could not validate the blueprint.'))
        } finally {
            if (!controller.signal.aborted) { operation.current = null; setBusy(null) }
        }
    }
    const document = !loading && draft?.blueprint.id === id ? draft : null
    return {document, catalog, loading, busy, error, conflict, report, edit, save, validate, reload,
        undo: () => travel('undo'), redo: () => travel('redo'), canUndo: history.current.past.length > 0, canRedo: history.current.future.length > 0,
        dirty: document !== null && JSON.stringify(document) !== JSON.stringify(baseline)}
}
