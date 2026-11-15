import {useEffect, useRef} from 'react'
import {Button, Div, Input, Label} from '../primitives'
import type {NodeCatalog, WorkflowNode} from '../../api/blueprints/types'
import {bindingText, inputBindings, nodeView} from '../../utils/blueprint-node-view'

interface Props {
    node: WorkflowNode | undefined
    catalog: NodeCatalog
    locked: boolean
    onName: (name: string) => void
    onClose: () => void
}

export const NodeInspector = ({node, catalog, locked, onName, onClose}: Props) => {
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
                            <Div key={name} className="blueprint-editor__value">
                                <span>{name}</span>
                                <code>{bindingText(input)}</code>
                            </Div>
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
                    </>
                ) : (
                    <Div className="blueprint-editor__empty">Select a node to inspect it.</Div>
                )}
            </fieldset>
        </aside>
    )
}
