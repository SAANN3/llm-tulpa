import {useEffect, useRef, useState} from 'react'
import type {BlueprintDocument, ControlEdge, InputBinding, NodeKind} from '../../api/blueprints/types'
import {
    controlConnectionError, createGraphNode, graphId, removeGraphNodes, copyGraphNodes, pasteGraphNodes, withControlEdges, withNodeInput,
    dataConnectionError, dataReferenceAt, replaceDataReference, literal,
    type GraphFragment, type GraphPoint, type GraphViewport, type NodeChoice,
} from '../../utils/blueprint-graph'
import {type DataLink} from '../../utils/blueprint-node-view'

interface BlueprintGraphOptions {
    document: BlueprintDocument | null
    locked: boolean
    edit: (update: (value: BlueprintDocument) => BlueprintDocument) => void
    onInspect: () => void
}

export const useBlueprintGraph = ({document, locked, edit, onInspect}: BlueprintGraphOptions) => {
    const [selection, setSelection] = useState<string[]>([])
    const clipboard = useRef<{fragment: GraphFragment; pastes: number} | null>(null)
    const [hasClipboard, setHasClipboard] = useState(false)
    const viewport = useRef<GraphViewport | null>(null)
    const [selectedEdge, setSelectedEdge] = useState<string | null>(null)
    const [selectedData, setSelectedData] = useState<DataLink | null>(null)
    const [graphMessage, setGraphMessage] = useState<string | null>(null)
    const [removing, setRemoving] = useState<{type: 'nodes'; ids: string[]} | {type: 'edge'; id: string} | {type: 'data'; link: DataLink} | null>(null)
    useEffect(() => {
        setSelection([])
        setSelectedEdge(null)
        setSelectedData(null)
        setRemoving(null)
        setGraphMessage(null)
    }, [document?.blueprint.id])

    const selectedNodes = document?.workflow.nodes.filter(item => selection.includes(item.id)) ?? []
    const selected = new Set(selectedNodes.map(item => item.id))
    const editableIds = selectedNodes.filter(item => item.kind.type !== 'entry').map(item => item.id)
    const node = selectedNodes.length === 1 ? selectedNodes[0] : undefined
    const edge = document?.workflow.edges.find(item => item.id === selectedEdge)
    const dataReference = document && selectedData ? dataReferenceAt(document.workflow, selectedData) : null
    const dataLink = dataReference ? selectedData : null
    const selectNodes = (ids: string[], inspect = false) => {
        setSelectedEdge(null)
        setSelectedData(null)
        setSelection(ids)
        if (inspect && ids.length === 1) onInspect()
    }
    const select = (id: string | null) => selectNodes(id === null ? [] : [id], true)
    const selectEdge = (edgeId: string) => {
        setSelection([])
        setSelectedData(null)
        setSelectedEdge(edgeId)
        onInspect()
    }
    const selectData = (link: DataLink) => {
        setSelection([])
        setSelectedEdge(null)
        setSelectedData(link)
        onInspect()
    }
    const addNode = (choice: NodeChoice, point?: GraphPoint) => {
        const position = viewport.current?.place(point)
        if (!document || locked || !position) return
        if (choice.type === 'builtin' && choice.definition.kind === 'entry' && document.workflow.nodes.some(item => item.kind.type === 'entry')) return
        const added = createGraphNode(choice, position, document.workflow)
        edit(value => ({...value, workflow: {...value.workflow, nodes: [...value.workflow.nodes, added]}}))
        select(added.id)
        setGraphMessage(null)
    }
    const connectControl = (from: string, port: string, to: string, replacing?: string) => {
        if (!document || locked) return false
        const message = controlConnectionError(document.workflow, from, port, to, replacing)
        if (message) { setGraphMessage(message); return false }
        if (replacing && document.workflow.edges.some(item => item.id === replacing && item.from === from && item.port === port && item.to === to)) {
            setGraphMessage(null)
            return true
        }
        const edgeId = replacing ?? graphId('edge')
        edit(value => {
            const edges = replacing ? value.workflow.edges.map(item => item.id === replacing ? {...item, from, port, to} : item)
                : [...value.workflow.edges, {id: edgeId, from, port, to}]
            return {...value, workflow: withControlEdges(value.workflow, edges)}
        })
        selectEdge(edgeId)
        setGraphMessage(null)
        return true
    }
    const bindInput = (from: string, port: string, to: string, name: string, replacing?: DataLink) => {
        if (!document || locked) return false
        const message = dataConnectionError(document.workflow, from, to, name)
        if (message) { setGraphMessage(message); return false }
        const source = document.workflow.nodes.find(item => item.id === from)!
        const original = replacing ? dataReferenceAt(document.workflow, replacing) : null
        if (replacing && !original) return false
        if (replacing?.node === to && replacing.input === name) {
            selectData(replacing)
            return true
        }
        const input: InputBinding = original ?? (source.kind.type === 'variable' && port === 'value'
            ? {source: 'variable', node_id: from} : {source: 'output', node_id: from, port})
        edit(value => {
            const workflow = replacing ? replaceDataReference(value.workflow, replacing, literal(null)) : value.workflow
            return {
                ...value,
                workflow: {...workflow, nodes: workflow.nodes.map(item => item.id === to ? withNodeInput(item, name, input) : item)},
            }
        })
        selectData({node: to, input: name, bindingPath: []})
        setGraphMessage(null)
        return true
    }
    const removeData = (link: DataLink) => {
        if (!document || locked || !dataReferenceAt(document.workflow, link)) return
        edit(value => ({...value, workflow: replaceDataReference(value.workflow, link, literal(null))}))
        setSelectedData(null)
    }
    const removeEdge = (edgeId: string) => {
        edit(value => ({...value, workflow: withControlEdges(value.workflow, value.workflow.edges.filter(item => item.id !== edgeId))}))
        setSelectedEdge(null)
    }
    const requestDelete = () => {
        if (locked) return
        if (edge) setRemoving({type: 'edge', id: edge.id})
        else if (dataLink) setRemoving({type: 'data', link: dataLink})
        else if (editableIds.length) setRemoving({type: 'nodes', ids: editableIds})
    }
    const deleteSelection = () => {
        if (!removing) return
        if (removing.type === 'edge') removeEdge(removing.id)
        else if (removing.type === 'data') removeData(removing.link)
        else {
            edit(value => ({...value, workflow: removeGraphNodes(value.workflow, new Set(removing.ids))}))
            setSelection([])
        }
    }
    const reorderEdge = (direction: -1 | 1) => {
        if (!edge) return
        edit(value => {
            const edges = [...value.workflow.edges]
            const siblings = edges.filter(item => item.from === edge.from && item.port === edge.port)
            const other = siblings[siblings.findIndex(item => item.id === edge.id) + direction]
            if (!other) return value
            const a = edges.findIndex(item => item.id === edge.id)
            const b = edges.findIndex(item => item.id === other.id)
            const previous = edges[a]
            edges[a] = edges[b]
            edges[b] = previous
            return {...value, workflow: {...value.workflow, edges}}
        })
    }
    const moveNodes = (ids: ReadonlySet<string>, delta: GraphPoint) => {
        if (locked || (delta.x === 0 && delta.y === 0)) return
        edit(value => ({
            ...value,
            workflow: {
                ...value.workflow,
                nodes: value.workflow.nodes.map(item => ids.has(item.id)
                    ? {...item, position: {x: item.position.x + delta.x, y: item.position.y + delta.y}} : item),
            },
        }))
    }
    const copySelection = (cut = false) => {
        if (!document || locked || editableIds.length === 0) return false
        const ids = new Set(editableIds)
        clipboard.current = {fragment: copyGraphNodes(document.workflow, ids), pastes: 0}
        setHasClipboard(true)
        if (cut) {
            edit(value => ({...value, workflow: removeGraphNodes(value.workflow, ids)}))
            selectNodes(selection.filter(id => !ids.has(id)))
        }
        return true
    }
    const pasteSelection = () => {
        const content = clipboard.current
        const anchor = viewport.current?.place()
        if (!document || locked || !content || !anchor) return false
        const step = 24 * (++content.pastes)
        const left = Math.min(...content.fragment.nodes.map(node => node.position.x))
        const top = Math.min(...content.fragment.nodes.map(node => node.position.y))
        const {nodes, edges} = pasteGraphNodes(content.fragment, {x: anchor.x - left + step, y: anchor.y - top + step})
        edit(value => ({
            ...value,
            workflow: {...value.workflow, nodes: [...value.workflow.nodes, ...nodes], edges: [...value.workflow.edges, ...edges]},
        }))
        selectNodes(nodes.map(item => item.id))
        return true
    }
    const selectAll = () => {
        if (!document) return false
        selectNodes(document.workflow.nodes.map(item => item.id))
        return true
    }
    const editKind = (kind: NodeKind, edges?: ControlEdge[]) => {
        if (!node) return
        edit(value => {
            const nodes = value.workflow.nodes.map(item => item.id === node.id ? {...item, kind} : item)
            return {...value, workflow: withControlEdges({...value.workflow, nodes}, edges ?? value.workflow.edges)}
        })
    }
    const renameNode = (name: string) => {
        edit(value => ({
            ...value,
            workflow: {
                ...value.workflow,
                nodes: value.workflow.nodes.map(item => item.id === node?.id ? {...item, name} : item),
            },
        }))
    }
    return {
        selected, selectedEdge, node, edge, dataLink, dataReference, viewport, graphMessage, removing,
        select, selectEdge, selectData, addNode, connectControl, bindInput,
        removeEdge, removeData, requestDelete, deleteSelection, reorderEdge, moveNodes, renameNode, editKind,
        selectNodes, selectAll, copySelection, pasteSelection, hasClipboard, canCopy: editableIds.length > 0,
        clearMessage: () => setGraphMessage(null),
        closeRemoval: () => setRemoving(null),
    }
}
