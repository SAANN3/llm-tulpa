import type {ControlEdge, Workflow} from '../../api/blueprints/types'
import type {DataLink, DataReference} from '../../utils/blueprint-node-view'
import {Button, Div, Label} from '../primitives'

type ConnectionInspectorProps = {
    workflow: Workflow
    locked: boolean
    onDelete: () => void
    onClose: () => void
} & (
    | {type: 'control'; edge: ControlEdge; onOrder: (direction: -1 | 1) => void}
    | {type: 'data'; link: DataLink; reference: DataReference}
)

export const ConnectionInspector = (props: ConnectionInspectorProps) => {
    const {workflow, locked, onDelete, onClose} = props
    const name = (id: string) => workflow.nodes.find(node => node.id === id)?.name ?? id
    const from = props.type === 'control' ? props.edge.from : props.reference.node_id
    const to = props.type === 'control' ? props.edge.to : props.link.node
    const port = props.type === 'control' ? props.edge.port : props.reference.source === 'output' ? props.reference.port : 'value'
    const order = props.type === 'control' ? {
        siblings: workflow.edges.filter(item => item.from === props.edge.from && item.port === props.edge.port),
        id: props.edge.id, onOrder: props.onOrder,
    } : null
    const index = order?.siblings.findIndex(item => item.id === order.id) ?? 0

    return (
        <aside className="blueprint-editor__inspector">
            <Div className="blueprint-editor__pane-heading">
                <span>{props.type === 'control' ? 'Control connection' : 'Data connection'}</span>
                <Button variant="secondary" onClicked={onClose}>×</Button>
            </Div>
            <fieldset disabled={locked}>
                <Label className="section-heading" text="From"/>
                <p>{name(from)} · {port}</p>
                {props.type === 'data' && props.reference.path?.length ? <p>Value path: {JSON.stringify(props.reference.path)}</p> : null}
                <Label className="section-heading" text="To"/>
                <p>{name(to)}{props.type === 'data' ? ` · ${props.link.input}` : ''}</p>
                {props.type === 'data' && props.link.bindingPath.length ? (
                    <p>Nested binding: {JSON.stringify(props.link.bindingPath)}</p>
                ) : null}
                {order ? (
                    <>
                        <Label className="section-heading" text="Execution order"/>
                        <p>{index + 1} of {order.siblings.length} from this output</p>
                        <Div className="blueprint-input__buttons">
                            <Button variant="secondary" text="Move up" disabled={locked || index === 0} onClicked={() => order.onOrder(-1)}/>
                            <Button variant="secondary" text="Move down" disabled={locked || index === order.siblings.length - 1} onClicked={() => order.onOrder(1)}/>
                        </Div>
                    </>
                ) : null}
                <p>Drag the endpoint to reconnect it, or drop it on empty canvas to remove it.</p>
                <Button variant="secondary" text="Delete connection" onClicked={onDelete}/>
            </fieldset>
        </aside>
    )
}
