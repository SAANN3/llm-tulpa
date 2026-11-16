import {useEffect, useRef} from 'react'
import {NodeConfig} from './node-forms/node-config'
import {Button, Div, Input, Label} from '../primitives'
import type {AvailableTool} from '../../api/chats/available-tools'
import type {LaunchProfile} from '../../api/profiles/types'
import type {ManagedModel} from '../../api/runtime/types'
import type {ControlEdge, NodeCatalog, NodeKind, ValidationReport, Workflow, WorkflowNode} from '../../api/blueprints/types'
import {nodeView} from '../../utils/blueprint-node-view'

interface Props {
    node: WorkflowNode | undefined
    catalog: NodeCatalog
    locked: boolean
    onName: (name: string) => void
    onClose: () => void
    workflow: Workflow
    tools: AvailableTool[]
    toolsLoaded: boolean
    models: ManagedModel[]
    profiles: LaunchProfile[]
    report: ValidationReport | null
    onKind: (kind: NodeKind, edges?: ControlEdge[]) => void
    onDelete: () => void
}

export const NodeInspector = ({node, catalog, locked, onName, onClose, workflow, tools, toolsLoaded, models, profiles, report, onKind, onDelete}: Props) => {
    const body = useRef<HTMLFieldSetElement>(null)
    const descriptor = node ? nodeView(node, catalog) : null

    useEffect(() => {
        if (body.current) body.current.scrollTop = 0
    }, [node?.id])

    return (
        <aside className="blueprint-editor__inspector">
            <Div className="blueprint-editor__pane-heading">
                <span>{descriptor?.title ?? 'Node inspector'}</span>
                <Button variant="secondary" onClicked={onClose}>×</Button>
            </Div>
            <fieldset ref={body} disabled={locked}>
                {node && descriptor ? (
                    <>
                        <label className="blueprint-editor__field">
                            <span>Node name</span>
                            <Input text={node.name} onChanged={onName}/>
                        </label>
                        {descriptor.unavailable ? (
                            <p className="blueprint-editor__warning">
                                This node definition is unavailable. Its saved configuration is preserved.
                            </p>
                        ) : null}
                        {(report?.diagnostics ?? []).filter(item => item.node_id === node.id).map(item => (
                            <p key={`${item.field}-${item.code}`} className="blueprint-editor__warning">{item.field}: {item.message}</p>
                        ))}
                        <NodeConfig node={node} workflow={workflow} catalog={catalog} tools={tools} toolsLoaded={toolsLoaded}
                                    models={models} profiles={profiles} onChange={onKind}/>
                        <Label className="section-heading" text="Outputs"/>
                        {descriptor.outputs.map(port => (
                            <Div key={port.name} className="blueprint-editor__value">
                                <span>{port.name}</span>
                                <code>{port.type}</code>
                            </Div>
                        ))}
                        <Label className="section-heading" text="Control outputs"/>
                        {descriptor.control.map(port => (
                            <Div key={port} className="blueprint-editor__value">{port}</Div>
                        ))}
                        <small className="blueprint-editor__node-id">id: {node.id}</small>
                        <Button variant="secondary" text="Delete node" disabled={locked || node.kind.type === 'entry'} onClicked={onDelete}/>
                    </>
                ) : (
                    <Div className="blueprint-editor__empty">Select a node to inspect it.</Div>
                )}
            </fieldset>
        </aside>
    )
}
