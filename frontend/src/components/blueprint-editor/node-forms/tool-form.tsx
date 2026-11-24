import type {AvailableTool} from '../../../api/chats/available-tools'
import type {NodeCatalog, NodeOf, Workflow} from '../../../api/blueprints/types'
import {literalBinding} from '../../../utils/blueprint-schema'
import {ArgumentFields, Check, Details} from './fields'
import {Label, Select} from '../../primitives'

interface ToolFormProps {
    node: NodeOf<'tool'>
    workflow: Workflow
    catalog: NodeCatalog
    tools: AvailableTool[]
    toolsLoaded: boolean
    onChange: (kind: NodeOf<'tool'>['kind']) => void
}

export const ToolForm = ({node, workflow, catalog, tools, toolsLoaded, onChange}: ToolFormProps) => {
    const kind = node.kind
    const known = tools.some(tool => tool.name === kind.name)
    const names = tools.map(tool => tool.name)
    if (!known) names.unshift(kind.name)
    const selected = tools.find(tool => tool.name === kind.name)
    const choose = (name: string) => {
        if (name === kind.name) return
        const tool = tools.find(item => item.name === name)
        const arguments_: typeof kind.arguments = {}
        if (tool) {
            for (const property of tool.properties) {
                if (Object.hasOwn(kind.arguments, property.name)) arguments_[property.name] = kind.arguments[property.name]
                else if (property.required) arguments_[property.name] = literalBinding(property)
            }
        }
        onChange({...kind, name, arguments: arguments_})
    }
    return (
        <>
            <Label className="section-heading" text="Tool"/>
            <Select values={names.length ? names : [kind.name]} selected={kind.name} onChosen={choose}/>
            {toolsLoaded && !known ? (
                <p className="blueprint-editor__warning">
                    {kind.name === 'unconfigured'
                        ? 'Choose a tool before this node can run. Saved arguments stay with the node.'
                        : `${kind.name} is not in this account's tools. Its arguments are kept.`}
                </p>
            ) : null}
            {selected?.description ? <Details key={kind.name} text={selected.description}/> : null}
            <ArgumentFields properties={selected?.properties ?? []} schemaKnown={!!selected} values={kind.arguments} workflow={workflow}
                            catalog={catalog} nodeId={node.id}
                            onChange={arguments_ => onChange({...kind, arguments: arguments_})}/>
            <Check label="Ask before this tool runs" on={!!kind.require_confirmation}
                   onToggle={require_confirmation => onChange({...kind, require_confirmation})}/>
        </>
    )
}
