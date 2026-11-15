import {useEffect, useRef, useState, type PointerEvent, type RefObject} from 'react'
import {Frame} from '../frame'
import {Button, Div} from '../primitives'
import type {NodeCatalog, Workflow} from '../../api/blueprints/types'
import {inputBindings, nodeView, references} from '../../utils/blueprint-node-view'

const NODE_WIDTH = 256
const PORT_ROW = 24
const CONTROL_TOP = 64
const SECTION_GAP = 26
const CANVAS_MARGIN = 64
const MIN_ZOOM = 0.15
const MAX_ZOOM = 1.7
const curve = (a: {x: number; y: number}, b: {x: number; y: number}) => {
    const bend = Math.max(60, Math.abs(b.x - a.x) * 0.45)
    return `M ${a.x} ${a.y} C ${a.x + bend} ${a.y}, ${b.x - bend} ${b.y}, ${b.x} ${b.y}`
}

interface Props {
    workflow: Workflow
    catalog: NodeCatalog
    selected: string | null
    onSelect: (id: string | null) => void
    onMove: (id: string, position: {x: number; y: number}) => void
    fitRef: RefObject<(() => void) | null>
    locked: boolean
}

type Gesture =
    | {type: 'pan'; pointer: number; clientX: number; clientY: number; x: number; y: number}
    | {type: 'node'; pointer: number; clientX: number; clientY: number; x: number; y: number; id: string; zoom: number}

export const GraphCanvas = ({workflow, catalog, selected, onSelect, onMove, fitRef, locked}: Props) => {
    const surface = useRef<HTMLDivElement>(null)
    const gesture = useRef<Gesture | null>(null)
    const [camera, setCamera] = useState({x: 0, y: 0, zoom: 1})
    const [drag, setDrag] = useState<{id: string; x: number; y: number} | null>(null)
    const [panning, setPanning] = useState(false)
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
        const dataTop = CONTROL_TOP + Math.max(control.length, 1) * PORT_ROW + SECTION_GAP
        return {...node, ...view, control, outputs, inputs, dataTop,
            position: drag?.id === node.id ? {x: drag.x, y: drag.y} : node.position,
            height: dataTop + Math.max(inputs.length, outputs.length) * PORT_ROW + 20}
    })
    const byId = new Map(nodes.map(node => [node.id, node]))
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
    fitRef.current = fit
    useEffect(() => {
        const frame = requestAnimationFrame(() => fitRef.current?.())
        return () => cancelAnimationFrame(frame)
    }, [fitRef])
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
        const active = gesture.current
        if (!active || event.pointerId !== active.pointer) return
        const dx = event.clientX - active.clientX, dy = event.clientY - active.clientY
        if (active.type === 'pan') setCamera(value => ({...value, x: active.x + dx, y: active.y + dy}))
        else setDrag({id: active.id, x: active.x + dx / active.zoom, y: active.y + dy / active.zoom})
    }
    const finish = () => {
        if (drag && gesture.current?.type === 'node') onMove(drag.id, {x: drag.x, y: drag.y})
        gesture.current = null
        setDrag(null)
        setPanning(false)
    }
    const cancel = () => {
        gesture.current = null
        setDrag(null)
        setPanning(false)
    }
    return (
        <div className={`blueprint-canvas${panning ? ' blueprint-canvas--panning' : ''}`} ref={surface}
             onPointerDown={event => {
                 if (event.button !== 0 || (event.target as Element).closest('article,button')) return
                 onSelect(null)
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
                    {workflow.edges.map(edge => {
                        const from = byId.get(edge.from)
                        const to = byId.get(edge.to)
                        if (!from || !to) return null
                        const a = {
                            x: from.position.x + NODE_WIDTH,
                            y: from.position.y + CONTROL_TOP + Math.max(0, from.control.indexOf(edge.port)) * PORT_ROW,
                        }
                        const b = {x: to.position.x, y: to.position.y + CONTROL_TOP}
                        return <path key={edge.id} d={curve(a, b)} className="blueprint-canvas__control-wire"/>
                    })}
                    {nodes.flatMap(node => node.inputs.flatMap(([name, input], inputIndex) => references(input).map((ref, index) => {
                        const from = byId.get(ref.node)
                        if (!from) return null
                        const outputIndex = Math.max(0, from.outputs.findIndex(port => port.name === ref.port))
                        const a = {
                            x: from.position.x + NODE_WIDTH,
                            y: from.position.y + from.dataTop + outputIndex * PORT_ROW,
                        }
                        const b = {
                            x: node.position.x,
                            y: node.position.y + node.dataTop + inputIndex * PORT_ROW,
                        }
                        return <path key={`${node.id}-${name}-${index}`} className="blueprint-canvas__data-wire" d={curve(a, b)}/>
                    })))}
                </svg>
                {nodes.map(node => (
                    <article key={node.id}
                             className={`blueprint-canvas__node${selected === node.id ? ' blueprint-canvas__node--selected' : ''}`}
                             aria-label={`Node ${node.name}`} tabIndex={0}
                             style={{left: node.position.x, top: node.position.y, width: NODE_WIDTH, height: node.height}}
                             onKeyDown={event => { if (event.key === 'Enter') onSelect(node.id) }}
                             onPointerDown={event => {
                                 if (event.button !== 0) return
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
                                <span key={port} className="blueprint-canvas__flow-port"
                                      style={{top: CONTROL_TOP + index * PORT_ROW}}>
                                    {port} ▷
                                </span>
                            ))}
                            {node.kind.type !== 'entry' ? (
                                <span className="blueprint-canvas__flow-input" style={{top: CONTROL_TOP}}>▷</span>
                            ) : null}
                            {node.inputs.map(([name], index) => (
                                <span key={name} className="blueprint-canvas__data-input"
                                      style={{top: node.dataTop + index * PORT_ROW}}>
                                    □ {name}
                                </span>
                            ))}
                            {node.outputs.map((port, index) => (
                                <span key={port.name} className="blueprint-canvas__data-output" title={port.type}
                                      style={{top: node.dataTop + index * PORT_ROW}}>
                                    {port.name} □
                                </span>
                            ))}
                        </Frame>
                    </article>
                ))}
            </div>
            <Div className="blueprint-canvas__legend">─ control · ┄ data</Div>
            <Div className="blueprint-canvas__controls">
                <Button variant="secondary" title="Zoom out" onClicked={() => zoomBy(0.8)}>−</Button>
                <span>{Math.round(camera.zoom * 100)}%</span>
                <Button variant="secondary" title="Zoom in" onClicked={() => zoomBy(1.25)}>+</Button>
                <Button variant="secondary" onClicked={fit}>Fit graph</Button>
            </Div>
        </div>
    )
}
