import {useEffect, useRef} from 'react'
import {InputBindingEditor} from './input-binding-editor'
import {Button, Div, Input, Label} from '../primitives'
import type {InputBinding, NodeCatalog, Workflow, WorkflowNode} from '../../api/blueprints/types'
import {inputBindings, nodeView} from '../../utils/blueprint-node-view'

interface Props {
    node: WorkflowNode | undefined
    catalog: NodeCatalog
    locked: boolean
    onName: (name: string) => void
    onClose: () => void
    workflow: Workflow
    onInput: (name: string, input: InputBinding) => void
    onDelete: () => void
}

export const NodeInspector = ({node, catalog, locked, onName, onClose, workflow, onInput, onDelete}: Props) => {
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
                        <Label className="section-heading" text="Inputs"/>
                        {inputBindings(node).map(([name, input]) => (
                            <InputBindingEditor key={name} name={name} input={input} workflow={workflow} catalog={catalog}
                                                nodeId={node.id} onChange={value => onInput(name, value)}/>
                        ))}
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
                        {node.kind.type === 'tool' && node.kind.name === 'unconfigured' ? (
                            <p className="blueprint-editor__warning">Choose a tool before running this node. Tool configuration is part of the next editor batch.</p>
                        ) : null}
                    </>
                ) : (
                    <Div className="blueprint-editor__empty">Select a node to inspect it.</Div>
                )}
            </fieldset>
        </aside>
    )
}
