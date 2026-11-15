import {useCallback, useEffect, useRef, useState, type PointerEvent, type RefObject} from 'react'
import type {Workflow} from '../../api/blueprints/types'
import {controlConnectionError, dataConnectionError, dataReferenceAt, type GraphPoint} from '../../utils/blueprint-graph'
import type {DataLink, DataReference} from '../../utils/blueprint-node-view'

export type PendingConnection =
    | {type: 'control'; node: string; port: string; edge?: string}
    | {type: 'data'; node: string; port: string; replacing?: {link: DataLink; original: DataReference}}

type Target = {type: 'control'; node: string; index: number} | {type: 'data'; node: string; input: string}

interface ConnectionOptions {
    workflow: Workflow
    surface: RefObject<HTMLDivElement | null>
    toWorld: (point: GraphPoint) => GraphPoint
    locked: boolean
    onControl: (from: string, port: string, to: string, replacing?: string) => boolean
    onData: (from: string, port: string, to: string, input: string, replacing?: DataLink) => boolean
    onSelectEdge: (id: string) => void
    onSelectData: (link: DataLink) => void
    onRemoveEdge: (id: string) => void
    onRemoveData: (link: DataLink) => void
}

const DRAG_THRESHOLD = 6

export const useBlueprintConnections = ({
    workflow, surface, toWorld, locked, onControl, onData,
    onSelectEdge, onSelectData, onRemoveEdge, onRemoveData,
}: ConnectionOptions) => {
    const [pending, setPending] = useState<PendingConnection | null>(null)
    const [point, setPoint] = useState<GraphPoint | null>(null)
    const [target, setTarget] = useState<Target | null>(null)
    const drag = useRef<{
        connection: PendingConnection; pointer: number; start: GraphPoint; moved: boolean; existing: boolean
    } | null>(null)
    const cancel = useCallback(() => {
        drag.current = null
        setPending(null)
        setPoint(null)
        setTarget(null)
    }, [])

    useEffect(() => {
        const missingNode = pending && !workflow.nodes.some(node => node.id === pending.node)
        const missingEdge = pending?.type === 'control' && pending.edge && !workflow.edges.some(edge => edge.id === pending.edge)
        const changedBinding = pending?.type === 'data' && pending.replacing
            && dataReferenceAt(workflow, pending.replacing.link) !== pending.replacing.original
        if (locked || missingNode || missingEdge || changedBinding) cancel()
    }, [locked, pending, workflow, cancel])
    useEffect(() => {
        const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') cancel() }
        window.addEventListener('keydown', escape)
        window.addEventListener('blur', cancel)
        return () => {
            window.removeEventListener('keydown', escape)
            window.removeEventListener('blur', cancel)
        }
    }, [cancel])

    const start = (next: PendingConnection) => {
        if (locked) return
        setPending(pending?.type === next.type && pending.node === next.node && pending.port === next.port ? null : next)
        setPoint(null)
        setTarget(null)
    }
    const begin = (event: PointerEvent, connection: PendingConnection, existing = false) => {
        if (event.button !== 0 || locked) return
        event.stopPropagation()
        drag.current = {connection, pointer: event.pointerId, start: {x: event.clientX, y: event.clientY}, moved: false, existing}
        if (existing) {
            event.preventDefault()
            setPending(connection)
            setPoint(toWorld({x: event.clientX, y: event.clientY}))
        }
        // Capture on the stable surface: the original endpoint disappears while the preview is drawn.
        surface.current?.setPointerCapture(event.pointerId)
    }
    const targetAt = (point: GraphPoint, connection: PendingConnection): Target | null => {
        const element = document.elementFromPoint(point.x, point.y)?.closest<HTMLElement>('[data-control-input], [data-data-input]')
        if (!element || !surface.current?.contains(element)) return null
        if (connection.type === 'control' && element.dataset.controlInput) {
            const node = element.dataset.controlInput
            if (!controlConnectionError(workflow, connection.node, connection.port, node, connection.edge)) {
                return {type: 'control', node, index: Number(element.dataset.controlIndex ?? 0)}
            }
        }
        if (connection.type === 'data' && element.dataset.dataNode && element.dataset.dataInput != null) {
            const node = element.dataset.dataNode, input = element.dataset.dataInput
            if (!dataConnectionError(workflow, connection.node, node, input)) return {type: 'data', node, input}
        }
        return null
    }
    const commit = (connection: PendingConnection, target: Target) => {
        if (connection.type === 'control' && target.type === 'control') return onControl(connection.node, connection.port, target.node, connection.edge)
        if (connection.type === 'data' && target.type === 'data') return onData(connection.node, connection.port, target.node, target.input, connection.replacing?.link)
        return false
    }
    const controlInput = (to: string) => {
        if (!locked && pending?.type === 'control' && onControl(pending.node, pending.port, to, pending.edge)) cancel()
    }
    const dataInput = (to: string, input: string) => {
        if (!locked && pending?.type === 'data' && onData(pending.node, pending.port, to, input, pending.replacing?.link)) cancel()
    }
    const move = (event: PointerEvent) => {
        const active = drag.current
        if (active) {
            if (event.pointerId !== active.pointer) return true
            if (!active.moved && Math.hypot(event.clientX - active.start.x, event.clientY - active.start.y) < DRAG_THRESHOLD) return true
            active.moved = true
            setPending(active.connection)
        }
        const connection = active?.connection ?? pending
        if (!connection) return false
        const position = {x: event.clientX, y: event.clientY}
        setPoint(toWorld(position))
        setTarget(targetAt(position, connection))
        return true
    }
    const finish = (event: PointerEvent) => {
        const active = drag.current
        if (!active || active.pointer !== event.pointerId) return false
        drag.current = null
        if (!active.moved && !active.existing && !locked) {
            start(active.connection)
            return true
        }
        if (!locked) {
            const connection = active.connection
            // Selecting on release keeps the mobile inspector from covering a drag in progress.
            if (!active.moved) {
                if (connection.type === 'control' && connection.edge) onSelectEdge(connection.edge)
                if (connection.type === 'data' && connection.replacing) onSelectData(connection.replacing.link)
            } else {
                const position = {x: event.clientX, y: event.clientY}
                const target = targetAt(position, connection)
                if (target) commit(connection, target)
                else if (active.existing) {
                    const element = document.elementFromPoint(position.x, position.y)
                    if (element?.closest('.blueprint-canvas') === surface.current && !element.closest('article,button')) {
                        if (connection.type === 'control' && connection.edge) onRemoveEdge(connection.edge)
                        if (connection.type === 'data' && connection.replacing) onRemoveData(connection.replacing.link)
                    }
                }
            }
        }
        cancel()
        return true
    }
    return {pending, point, target, start, begin, controlInput, dataInput, move, finish, cancel}
}
