import {useEffect, useRef, useState} from 'react'
import type {BlueprintDocument, ControlEdge, InputBinding, NodeKind} from '../../api/blueprints/types'
import {
    controlConnectionError, createGraphNode, graphId, removeGraphNode, withControlEdges, withNodeInput,
    dataConnectionError, dataReferenceAt, replaceDataReference, literal,
    type GraphPoint, type GraphViewport, type NodeChoice,
} from '../../utils/blueprint-graph'
import {type DataLink} from '../../utils/blueprint-node-view'

interface BlueprintGraphOptions {
    document: BlueprintDocument | null
    locked: boolean
    edit: (update: (value: BlueprintDocument) => BlueprintDocument) => void
    onInspect: () => void
}

export const useBlueprintGraph = ({document, locked, edit, onInspect}: BlueprintGraphOptions) => {
    const [selected, setSelected] = useState<string | null>(null)
    const viewport = useRef<GraphViewport | null>(null)
    const [selectedEdge, setSelectedEdge] = useState<string | null>(null)
    const [selectedData, setSelectedData] = useState<DataLink | null>(null)
    const [graphMessage, setGraphMessage] = useState<string | null>(null)
    const [removing, setRemoving] = useState<{type: 'node' | 'edge'; id: string} | {type: 'data'; link: DataLink} | null>(null)
    useEffect(() => {
        setSelected(null)
        setSelectedEdge(null)
        setSelectedData(null)
        setRemoving(null)
        setGraphMessage(null)
    }, [document?.blueprint.id])

    const node = document?.workflow.nodes.find(item => item.id === selected)
    const edge = document?.workflow.edges.find(item => item.id === selectedEdge)
    const dataReference = document && selectedData ? dataReferenceAt(document.workflow, selectedData) : null
    const dataLink = dataReference ? selectedData : null
    const select = (nodeId: string | null) => {
        setSelectedEdge(null)
        setSelectedData(null)
        setSelected(nodeId)
        if (nodeId) {
            onInspect()
        }
    }
    const selectEdge = (edgeId: string) => {
        setSelected(null)
        setSelectedData(null)
        setSelectedEdge(edgeId)
        onInspect()
    }
    const selectData = (link: DataLink) => {
        setSelected(null)
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
        else if (node && node.kind.type !== 'entry') setRemoving({type: 'node', id: node.id})
    }
    const deleteSelection = () => {
        if (!removing) return
        if (removing.type === 'edge') removeEdge(removing.id)
        else if (removing.type === 'data') removeData(removing.link)
        else {
            edit(value => ({...value, workflow: removeGraphNode(value.workflow, removing.id)}))
            setSelected(null)
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
    const moveNode = (nodeId: string, position: {x: number; y: number}) => {
        edit(value => ({
            ...value,
            workflow: {
                ...value.workflow,
                nodes: value.workflow.nodes.map(item => item.id === nodeId ? {...item, position} : item),
            },
        }))
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
        removeEdge, removeData, requestDelete, deleteSelection, reorderEdge, moveNode, renameNode, editKind,
        clearMessage: () => setGraphMessage(null),
        closeRemoval: () => setRemoving(null),
    }
}
