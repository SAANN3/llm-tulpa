import {useEffect, useRef, useState, type PointerEvent, type RefObject} from 'react'
import {Frame} from '../frame'
import {Button, Div} from '../primitives'
import type {NodeCatalog, Workflow} from '../../api/blueprints/types'
import {inputBindings, nodeView, references, sameDataLink, type DataLink} from '../../utils/blueprint-node-view'
import {useBlueprintConnections} from '../../hooks/blueprint-editor/use-blueprint-connections'
import type {GraphPoint, GraphViewport} from '../../utils/blueprint-graph'

const NODE_WIDTH = 256
const PORT_ROW = 24
const CONTROL_TOP = 64
const SECTION_GAP = 26
const CANVAS_MARGIN = 64
const MIN_ZOOM = 0.15
const MAX_ZOOM = 1.7
const curveBend = (a: GraphPoint, b: GraphPoint) => Math.max(60, Math.abs(b.x - a.x) * 0.45)
const curve = (a: GraphPoint, b: GraphPoint) => {
    const bend = curveBend(a, b)
    return `M ${a.x} ${a.y} C ${a.x + bend} ${a.y}, ${b.x - bend} ${b.y}, ${b.x} ${b.y}`
}

const curvePoint = (a: GraphPoint, b: GraphPoint, t: number): GraphPoint => {
    const bend = curveBend(a, b), u = 1 - t
    return {
        x: u ** 3 * a.x + 3 * u ** 2 * t * (a.x + bend) + 3 * u * t ** 2 * (b.x - bend) + t ** 3 * b.x,
        y: (u ** 3 + 3 * u ** 2 * t) * a.y + (3 * u * t ** 2 + t ** 3) * b.y,
    }
}

interface Props {
    workflow: Workflow
    catalog: NodeCatalog
    selected: string | null
    onSelect: (id: string | null) => void
    onMove: (id: string, position: {x: number; y: number}) => void
    viewportRef: RefObject<GraphViewport | null>
    selectedEdge: string | null
    onSelectEdge: (id: string) => void
    onControl: (from: string, port: string, to: string, replacing?: string) => boolean
    onData: (from: string, port: string, to: string, input: string, replacing?: DataLink) => boolean
    selectedData: DataLink | null
    onSelectData: (link: DataLink) => void
    onRemoveData: (link: DataLink) => void
    onRemoveEdge: (id: string) => void
    locked: boolean
}

type Gesture =
    | {type: 'pan'; pointer: number; clientX: number; clientY: number; x: number; y: number}
    | {type: 'node'; pointer: number; clientX: number; clientY: number; x: number; y: number; id: string; zoom: number}

export const GraphCanvas = ({
    workflow, catalog, selected, onSelect, onMove, viewportRef, locked,
    selectedEdge, onSelectEdge, onControl, onData, onRemoveEdge, selectedData, onSelectData, onRemoveData,
}: Props) => {
    const surface = useRef<HTMLDivElement>(null)
    const gesture = useRef<Gesture | null>(null)
    const [camera, setCamera] = useState({x: 0, y: 0, zoom: 1})
    const [drag, setDrag] = useState<{id: string; x: number; y: number} | null>(null)
    const [panning, setPanning] = useState(false)
    const toWorld = (point: GraphPoint): GraphPoint => {
        const rect = surface.current!.getBoundingClientRect()
        return {x: (point.x - rect.left - camera.x) / camera.zoom, y: (point.y - rect.top - camera.y) / camera.zoom}
    }
    const connections = useBlueprintConnections({
        workflow, surface, toWorld, locked, onControl, onData, onSelectEdge, onRemoveEdge, onSelectData, onRemoveData,
    })
    const allReferences = workflow.nodes.flatMap(node => inputBindings(node).flatMap(([, input]) => references(input)))
    const nodes = workflow.nodes.map(node => {
        const view = nodeView(node, catalog)
        // Preserve visible connections when an installed definition no longer declares their ports.
        const control = [...new Set([...view.control, ...workflow.edges.filter(edge => edge.from === node.id).map(edge => edge.port)])]
        const outputs = [...view.outputs]
        allReferences.filter(ref => ref.node === node.id).forEach(ref => {
            if (!outputs.some(port => port.name === ref.port)) outputs.push({name: ref.port, type: 'dynamic'})
        })
        const inputs = inputBindings(node)
        const incoming = workflow.edges.filter(edge => edge.to === node.id)
        const controlRows = Math.max(control.length, node.kind.type === 'merge' ? incoming.length : 1, 1)
        const dataTop = CONTROL_TOP + controlRows * PORT_ROW + SECTION_GAP
        return {...node, ...view, control, outputs, inputs, incoming, dataTop,
            position: drag?.id === node.id ? {x: drag.x, y: drag.y} : node.position,
            height: dataTop + Math.max(inputs.length, outputs.length) * PORT_ROW + 20}
    })
    const byId = new Map(nodes.map(node => [node.id, node]))
    const controlWires = workflow.edges.flatMap(edge => {
        const from = byId.get(edge.from), to = byId.get(edge.to)
        if (!from || !to) return []
        if (connections.pending?.type === 'control' && connections.pending.edge === edge.id && connections.point) return []
        const a = {
            x: from.position.x + NODE_WIDTH,
            y: from.position.y + CONTROL_TOP + Math.max(0, from.control.indexOf(edge.port)) * PORT_ROW,
        }
        const index = to.kind.type === 'merge' ? Math.max(0, to.incoming.findIndex(item => item.id === edge.id)) : 0
        const b = {x: to.position.x, y: to.position.y + CONTROL_TOP + index * PORT_ROW}
        return [{edge, a, b, path: curve(a, b)}]
    })
    const dataWires = nodes.flatMap(node => node.inputs.flatMap(([name, input], inputIndex) => {
        const refs = references(input)
        return refs.flatMap((ref, index) => {
            const from = byId.get(ref.node)
            if (!from) return []
            const link: DataLink = {node: node.id, input: name, bindingPath: ref.bindingPath}
            if (connections.pending?.type === 'data' && sameDataLink(connections.pending.replacing?.link, link) && connections.point) return []
            const outputIndex = Math.max(0, from.outputs.findIndex(port => port.name === ref.port))
            const a = {x: from.position.x + NODE_WIDTH, y: from.position.y + from.dataTop + outputIndex * PORT_ROW}
            const b = {x: node.position.x, y: node.position.y + node.dataTop + inputIndex * PORT_ROW}
            // Separate handles along a shared curve so repeated nested references remain individually selectable.
            const handle = refs.length === 1 ? {x: b.x - 9, y: b.y} : curvePoint(a, b, 0.35 + index * 0.5 / (refs.length - 1))
            return [{key: JSON.stringify(link), link, reference: ref.binding, from: ref.node, port: ref.port, handle, path: curve(a, b)}]
        })
    }))
    const fit = () => {
        if (!surface.current || !nodes.length) return
        const left = Math.min(...nodes.map(node => node.position.x))
        const top = Math.min(...nodes.map(node => node.position.y))
        const right = Math.max(...nodes.map(node => node.position.x + NODE_WIDTH))
        const bottom = Math.max(...nodes.map(node => node.position.y + node.height))
        const {width, height} = surface.current.getBoundingClientRect()
        const zoom = Math.max(MIN_ZOOM, Math.min(1, (width - CANVAS_MARGIN * 2) / (right - left), (height - CANVAS_MARGIN * 2) / (bottom - top)))
        setCamera({zoom, x: (width - (right - left) * zoom) / 2 - left * zoom, y: (height - (bottom - top) * zoom) / 2 - top * zoom})
    }
    viewportRef.current = {
        fit,
        place: point => {
            const element = surface.current
            if (!element) return null
            const rect = element.getBoundingClientRect()
            if (point && document.elementFromPoint(point.x, point.y)?.closest('.blueprint-canvas') !== element) return null
            const center = toWorld(point ?? {x: rect.left + rect.width / 2, y: rect.top + rect.height / 2})
            return {x: center.x - NODE_WIDTH / 2, y: center.y - CONTROL_TOP}
        },
    }
    useEffect(() => {
        const frame = requestAnimationFrame(() => viewportRef.current?.fit())
        return () => cancelAnimationFrame(frame)
    }, [viewportRef])
    useEffect(() => {
        if (locked) { gesture.current = null; setDrag(null); setPanning(false) }
    }, [locked])
    useEffect(() => {
        const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') { gesture.current = null; setDrag(null); setPanning(false) } }
        window.addEventListener('keydown', escape)
        return () => window.removeEventListener('keydown', escape)
    }, [])
    const zoomBy = (factor: number, anchor?: {x: number; y: number}) => {
        setCamera(previous => {
            const zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, previous.zoom * factor))
            const point = anchor ?? {x: (surface.current?.clientWidth ?? 0) / 2, y: (surface.current?.clientHeight ?? 0) / 2}
            return {zoom, x: point.x - (point.x - previous.x) * zoom / previous.zoom, y: point.y - (point.y - previous.y) * zoom / previous.zoom}
        })
    }
    useEffect(() => {
        const element = surface.current
        if (!element) return
        const wheel = (event: WheelEvent) => {
            event.preventDefault()
            const bounds = element.getBoundingClientRect()
            zoomBy(event.deltaY > 0 ? 0.92 : 1.08, {x: event.clientX - bounds.left, y: event.clientY - bounds.top})
        }
        element.addEventListener('wheel', wheel, {passive: false})
        return () => element.removeEventListener('wheel', wheel)
    }, [])
    const move = (event: PointerEvent<HTMLDivElement>) => {
        if (connections.move(event)) return
        const active = gesture.current
        if (!active || event.pointerId !== active.pointer) return
        const dx = event.clientX - active.clientX, dy = event.clientY - active.clientY
        if (active.type === 'pan') setCamera(value => ({...value, x: active.x + dx, y: active.y + dy}))
        else setDrag({id: active.id, x: active.x + dx / active.zoom, y: active.y + dy / active.zoom})
    }
    const finish = (event: PointerEvent<HTMLDivElement>) => {
        if (connections.finish(event)) return
        if (drag && gesture.current?.type === 'node') onMove(drag.id, {x: drag.x, y: drag.y})
        gesture.current = null
        setDrag(null)
        setPanning(false)
    }
    const cancel = () => {
        connections.cancel()
        gesture.current = null
        setDrag(null)
        setPanning(false)
    }
    return (
        <div className={`blueprint-canvas${panning ? ' blueprint-canvas--panning' : ''}`} ref={surface}
             onPointerDown={event => {
                 if (event.button !== 0 || (event.target as Element).closest('article,button,[data-edge],[data-data-link],[data-data-link-handle]')) return
                 onSelect(null)
                 connections.cancel()
                 gesture.current = {
                     type: 'pan', pointer: event.pointerId,
                     clientX: event.clientX, clientY: event.clientY,
                     x: camera.x, y: camera.y,
                 }
                 event.currentTarget.setPointerCapture(event.pointerId)
                 setPanning(true)
             }}
             onPointerMove={move} onPointerUp={finish} onPointerCancel={cancel}
             style={{
                 backgroundPosition: `${camera.x}px ${camera.y}px`,
                 backgroundSize: `${Math.max(12, 24 * camera.zoom)}px ${Math.max(12, 24 * camera.zoom)}px`,
             }}>
            <div className="blueprint-canvas__world"
                 style={{transform: `translate(${camera.x}px,${camera.y}px) scale(${camera.zoom})`}}>
                <svg className="blueprint-canvas__wires" width={1} height={1} aria-label="Node connections">
                    {controlWires.map(wire => (
                        <g key={wire.edge.id} data-edge={wire.edge.id} className={selectedEdge === wire.edge.id ? 'is-selected' : ''}>
                            <path d={wire.path} className="blueprint-canvas__control-wire"/>
                            <path d={wire.path} className="blueprint-canvas__wire-hit"
                                  onPointerDown={event => { event.stopPropagation(); onSelectEdge(wire.edge.id) }}/>
                        </g>
                    ))}
                    {dataWires.map(wire => (
                        <g key={wire.key} data-data-link={wire.key} className={sameDataLink(selectedData, wire.link) ? 'is-selected' : ''}>
                            <path d={wire.path} className="blueprint-canvas__data-wire"/>
                            <path d={wire.path} className="blueprint-canvas__wire-hit"
                                  onPointerDown={event => { event.stopPropagation(); onSelectData(wire.link) }}/>
                        </g>
                    ))}
                    {controlWires.map(wire => (
                        <g key={wire.edge.id} data-edge={wire.edge.id}>
                            <circle cx={wire.b.x - 9} cy={wire.b.y} r={7} className="blueprint-canvas__wire-handle"
                                    onPointerDown={event => connections.begin(event, {
                                        type: 'control', node: wire.edge.from, port: wire.edge.port, edge: wire.edge.id,
                                    }, true)}/>
                        </g>
                    ))}
                    {dataWires.map(wire => (
                        <g key={wire.key} data-data-link-handle={wire.key}>
                            <circle cx={wire.handle.x} cy={wire.handle.y} r={7}
                                    className="blueprint-canvas__wire-handle blueprint-canvas__wire-handle--data"
                                    onPointerDown={event => connections.begin(event, {
                                        type: 'data', node: wire.from, port: wire.port,
                                        replacing: {link: wire.link, original: wire.reference},
                                    }, true)}/>
                        </g>
                    ))}
                    {connections.pending && connections.point ? (() => {
                        const source = byId.get(connections.pending.node)
                        if (!source) return null
                        const index = connections.pending.type === 'control' ? source.control.indexOf(connections.pending.port)
                            : source.outputs.findIndex(port => port.name === connections.pending!.port)
                        const a = {
                            x: source.position.x + NODE_WIDTH,
                            y: source.position.y + (connections.pending.type === 'control' ? CONTROL_TOP : source.dataTop) + Math.max(0, index) * PORT_ROW,
                        }
                        return <path d={curve(a, connections.point)} className="blueprint-canvas__pending-wire"/>
                    })() : null}
                </svg>
                {nodes.map(node => (
                    <article key={node.id} data-node-id={node.id}
                             className={`blueprint-canvas__node${selected === node.id ? ' blueprint-canvas__node--selected' : ''}`}
                             aria-label={`Node ${node.name}`} tabIndex={0}
                             style={{left: node.position.x, top: node.position.y, width: NODE_WIDTH, height: node.height}}
                             onKeyDown={event => { if (event.key === 'Enter') onSelect(node.id) }}
                             onPointerDown={event => {
                                 if (event.button !== 0 || (event.target as Element).closest('button')) return
                                 event.stopPropagation()
                                 onSelect(node.id)
                                 if (locked) return
                                 gesture.current = {
                                     type: 'node', pointer: event.pointerId, id: node.id,
                                     clientX: event.clientX, clientY: event.clientY,
                                     x: node.position.x, y: node.position.y, zoom: camera.zoom,
                                 }
                                 event.currentTarget.setPointerCapture(event.pointerId)
                             }}>
                        <Frame title={node.name} actions={[]} active={false} bodyClassName="blueprint-canvas__node-body">
                            <span className="blueprint-canvas__kind">
                                {node.title}{node.unavailable ? ' · unavailable' : ''}
                            </span>
                            {node.control.map((port, index) => (
                                <Button key={port} variant="secondary" className="blueprint-canvas__flow-port"
                                        title={`Control output ${node.name}.${port}`} disabled={locked || node.unavailable}
                                        style={{top: CONTROL_TOP + index * PORT_ROW}}
                                        onPointerDown={event => connections.begin(event, {type: 'control', node: node.id, port})}
                                        onClicked={event => { if (event.detail === 0) connections.start({type: 'control', node: node.id, port}) }}>
                                    {port} ▷
                                </Button>
                            ))}
                            {node.kind.type !== 'entry' ? Array.from({length: node.kind.type === 'merge' ? Math.max(1, node.incoming.length) : 1}, (_, index) => (
                                <span key={index} data-control-input={node.id} data-control-index={index}>
                                    <Button variant="secondary"
                                            className={[
                                                'blueprint-canvas__flow-input',
                                                connections.target?.type === 'control' && connections.target.node === node.id
                                                    && connections.target.index === index ? 'is-connection-target' : null,
                                            ].filter(Boolean).join(' ')}
                                            title={`Control input ${node.name}${index ? ` (${index + 1})` : ''}`} disabled={locked}
                                            style={{top: CONTROL_TOP + index * PORT_ROW}}
                                            onClicked={() => connections.controlInput(node.id)}>▷</Button>
                                </span>
                            )) : null}
                            {node.inputs.map(([name], index) => (
                                <span key={name} data-data-node={node.id} data-data-input={name}>
                                    <Button variant="secondary"
                                            className={[
                                                'blueprint-canvas__data-input',
                                                connections.target?.type === 'data' && connections.target.node === node.id
                                                    && connections.target.input === name ? 'is-connection-target' : null,
                                            ].filter(Boolean).join(' ')}
                                            title={`Data input ${node.name}.${name}`} disabled={locked}
                                            style={{top: node.dataTop + index * PORT_ROW}}
                                            onClicked={() => connections.dataInput(node.id, name)}>
                                        □ {name}
                                    </Button>
                                </span>
                            ))}
                            {node.outputs.map((port, index) => (
                                <Button key={port.name} variant="secondary" className="blueprint-canvas__data-output"
                                        title={`Data output ${node.name}.${port.name} (${port.type})`} disabled={locked || node.unavailable}
                                        style={{top: node.dataTop + index * PORT_ROW}}
                                        onPointerDown={event => connections.begin(event, {type: 'data', node: node.id, port: port.name})}
                                        onClicked={event => { if (event.detail === 0) connections.start({type: 'data', node: node.id, port: port.name}) }}>
                                    {port.name} □
                                </Button>
                            ))}
                        </Frame>
                    </article>
                ))}
            </div>
            <Div className="blueprint-canvas__legend">
                {connections.pending ? 'Pick an input · Esc cancels' : '─ control · ┄ data · Drag or click an output'}
            </Div>
            <Div className="blueprint-canvas__controls">
                <Button variant="secondary" title="Zoom out" onClicked={() => zoomBy(0.8)}>−</Button>
                <span>{Math.round(camera.zoom * 100)}%</span>
                <Button variant="secondary" title="Zoom in" onClicked={() => zoomBy(1.25)}>+</Button>
                <Button variant="secondary" onClicked={fit}>Fit graph</Button>
            </Div>
        </div>
    )
}
