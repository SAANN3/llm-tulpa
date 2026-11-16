import type {NodeCatalog, NodeOf, Workflow, WorkflowNode} from '../../../api/blueprints/types'
import {ArgumentFields, Details} from './fields'
import {Label} from '../../primitives'

export const PluginForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'plugin'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
}) => {
    const kind = node.kind
    const registration = catalog.plugin_nodes.find(item =>
        item.plugin_name === kind.plugin_name && item.plugin_subname === kind.plugin_subname
        && item.node_type === kind.node_type && item.version === kind.version)
    return (
        <>
            <Label className="section-heading" text={registration?.title ?? kind.node_type}/>
            <small className="blueprint-form__hint">{kind.plugin_name}/{kind.plugin_subname} · {kind.node_type} v{kind.version}</small>
            {registration?.description ? <Details text={registration.description}/> : null}
            {registration ? null : (
                <p className="blueprint-editor__warning">This plugin node version is not available. Its arguments are kept.</p>
            )}
            <ArgumentFields properties={registration?.inputs ?? []} schemaKnown={!!registration} values={kind.arguments}
                            workflow={workflow} catalog={catalog} nodeId={node.id}
                            onChange={arguments_ => onChange({...kind, arguments: arguments_})}/>
        </>
    )
}
