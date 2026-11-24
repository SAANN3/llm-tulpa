import {useState} from 'react'
import type {InputBinding, NodeCatalog, Workflow} from '../../api/blueprints/types'
import {compatiblePorts, emptyBinding, fits, integerOnly, literalChoices, portInfo, schemaAccepts, type Accepted} from '../../utils/blueprint-accepts'
import {literal} from '../../utils/blueprint-graph'
import {nodeView} from '../../utils/blueprint-node-view'
import {Button, Div, Input, Select} from '../primitives'

interface InputBindingEditorProps {
    name: string
    input: InputBinding
    workflow: Workflow
    catalog: NodeCatalog
    nodeId: string
    onChange: (input: InputBinding) => void
    /** What this consumer accepts. Omitted when the node takes any JSON value. */
    accepts?: Accepted
    depth?: number
}

const sourceName = (input: InputBinding): string => {
    if (input.source !== 'literal') return {array: 'Array', object: 'Object', output: 'Output', variable: 'Variable'}[input.source]
    if (input.value === null) return 'Null'
    if (Array.isArray(input.value)) return 'Array'
    if (typeof input.value === 'string') return 'Text'
    if (typeof input.value === 'number') return 'Number'
    if (typeof input.value === 'boolean') return 'Boolean'
    return 'Object'
}

const NumericValue = ({value, onChange}: {value: number; onChange: (value: number) => void}) => {
    const [draft, setDraft] = useState<string | null>(null)
    return (
        <Input text={draft ?? String(value)} placeholder="Number" onChanged={text => {
            setDraft(text)
            if (text.trim() && Number.isFinite(Number(text))) onChange(Number(text))
        }} onBlur={() => setDraft(null)}/>
    )
}

export const InputBindingEditor = ({name, input, workflow, catalog, nodeId, onChange, accepts, depth = 0}: InputBindingEditorProps) => {
    const [field, setField] = useState('')
    const sources = workflow.nodes.filter(node => node.id !== nodeId).map(node => ({node, view: nodeView(node, catalog)}))
    const outputs = sources.filter(item => compatiblePorts(item.node, workflow, catalog, accepts).length || (input.source === 'output' && input.node_id === item.node.id))
    const variables = sources.filter(item => {
        if (item.node.kind.type !== 'variable') return false
        const info = portInfo(item.node, 'value', workflow, catalog)
        return fits(info.kind, info.schema, accepts) || (input.source === 'variable' && input.node_id === item.node.id)
    })
    const source = sourceName(input)
    const choices = literalChoices(accepts)
    if (outputs.length || input.source === 'output') choices.push('Output')
    if (variables.length || input.source === 'variable') choices.push('Variable')
    const choose = (choice: string) => {
        if (choice === source) return
        switch (choice) {
            case 'Text': onChange(literal('')); break
            case 'Number': onChange(literal(0)); break
            case 'Boolean': onChange(literal(false)); break
            case 'Null': onChange(literal(null)); break
            case 'Array': onChange({source: 'array', items: []}); break
            case 'Object': onChange({source: 'object', fields: {}}); break
            case 'Output': {
                const first = outputs[0]
                const port = first ? compatiblePorts(first.node, workflow, catalog, accepts)[0] ?? first.view.outputs[0]?.name : undefined
                if (first && port) onChange({source: 'output', node_id: first.node.id, port})
                break
            }
            case 'Variable': {
                const first = variables[0]
                if (first) onChange({source: 'variable', node_id: first.node.id})
                break
            }
        }
    }
    const collection = input.source === 'array' || input.source === 'object' ? input : input.source === 'literal' ? (
        Array.isArray(input.value) ? {source: 'array' as const, items: input.value.map(literal)}
            : input.value !== null && typeof input.value === 'object' ? {
                source: 'object' as const, fields: Object.fromEntries(Object.entries(input.value).map(([key, value]) => [key, literal(value)])),
            } : null
    ) : null
    const children = collection?.source === 'array' ? collection.items.map((value, index) => [String(index), value] as const)
        : collection?.source === 'object' ? Object.entries(collection.fields) : []
    const updateChild = (key: string, value: InputBinding | null) => {
        if (collection?.source === 'array') {
            onChange({...collection, items: value ? collection.items.map((item, index) => String(index) === key ? value : item)
                : collection.items.filter((_, index) => String(index) !== key)})
        } else if (collection?.source === 'object') {
            const fields = {...collection.fields}
            if (value) fields[key] = value
            else delete fields[key]
            onChange({...collection, fields})
        }
    }
    const referenceNodes = input.source === 'variable' ? variables : outputs
    const current = input.source === 'output' || input.source === 'variable' ? input.node_id : null
    const referenceOptions = referenceNodes.map(item => `${item.node.name} (${item.node.id})`)
    const currentSource = referenceNodes.find(item => item.node.id === current)
    const selectedSource = currentSource ? `${currentSource.node.name} (${current})` : `${current} (unavailable)`
    if (current && !currentSource) referenceOptions.unshift(selectedSource)

    return (
        <Div className="blueprint-input">
            <Div className="blueprint-input__heading">
                <span>{name}</span>
                <Select values={choices} selected={source} onChosen={choose}/>
            </Div>
            {input.source === 'literal' && !collection ? (
                typeof input.value === 'string' ? <Input text={input.value} onChanged={text => onChange(literal(text))}/>
                    : typeof input.value === 'number' ? <NumericValue value={input.value} onChange={value => {
                        if (integerOnly(accepts) && !Number.isInteger(value)) return
                        onChange(literal(value))
                    }}/>
                    : typeof input.value === 'boolean' ? <Select values={['false', 'true']} selected={String(input.value)} onChosen={value => onChange(literal(value === 'true'))}/>
                    : <small>No value</small>
            ) : null}
            {input.source === 'output' || input.source === 'variable' ? (
                <>
                    <Select values={referenceOptions} selected={selectedSource} onChosen={value => {
                        const item = referenceNodes.find(candidate => `${candidate.node.name} (${candidate.node.id})` === value)
                        if (!item || item.node.id === input.node_id) return
                        const port = item.view.outputs.length ? compatiblePorts(item.node, workflow, catalog, accepts)[0] ?? item.view.outputs[0].name : undefined
                        onChange(input.source === 'output' && port ? {...input, node_id: item.node.id, port, path: []}
                            : {...input, node_id: item.node.id, path: []})
                    }}/>
                    {input.source === 'output' ? (
                        <Select values={[...new Set([input.port, ...(currentSource ? compatiblePorts(currentSource.node, workflow, catalog, accepts) : [])])]}
                                selected={input.port} onChosen={port => { if (port !== input.port) onChange({...input, port, path: []}) }}/>
                    ) : null}
                    {(input.path ?? []).map((part, index) => (
                        <Div key={index} className="blueprint-input__path">
                            <span>{typeof part === 'number' ? 'Index' : 'Field'}</span>
                            <Input text={String(part)} onChanged={value => {
                                const next = typeof part === 'number' ? Number(value) : value
                                if (typeof next === 'number' && (!Number.isInteger(next) || next < 0 || next > 4294967295)) return
                                onChange({...input, path: input.path?.map((item, i) => i === index ? next : item)})
                            }}/>
                            <Button variant="secondary" text="Remove" onClicked={() => onChange({...input, path: input.path?.filter((_, i) => i !== index)})}/>
                        </Div>
                    ))}
                    <Div className="blueprint-input__buttons">
                        <Button variant="secondary" text="+ Field" onClicked={() => onChange({...input, path: [...(input.path ?? []), '']})}/>
                        <Button variant="secondary" text="+ Index" onClicked={() => onChange({...input, path: [...(input.path ?? []), 0]})}/>
                    </Div>
                </>
            ) : null}
            {collection ? (
                <>
                    {depth >= 32 ? <small>Input nesting limit reached.</small> : children.map(([key, value]) => {
                        const property = accepts?.fields?.find(item => item.name === key)
                        return (
                            <Div key={key} className="blueprint-input__child">
                                <InputBindingEditor name={key} input={value} workflow={workflow} catalog={catalog} nodeId={nodeId}
                                                    accepts={property ? schemaAccepts(property) : accepts?.items}
                                                    depth={depth + 1} onChange={next => updateChild(key, next)}/>
                                <Button variant="secondary" text="Remove" onClicked={() => updateChild(key, null)}/>
                            </Div>
                        )
                    })}
                    {depth < 32 ? collection.source === 'array' ? (
                        <Button variant="secondary" text="+ Item" onClicked={() => onChange({...collection, items: [...collection.items, emptyBinding(accepts?.items)]})}/>
                    ) : accepts?.fields?.filter(item => !Object.hasOwn(collection.fields, item.name)).length ? (
                        <Select values={accepts.fields.filter(item => !Object.hasOwn(collection.fields, item.name)).map(item => item.name)}
                                selected="" onChosen={name => {
                                    const property = accepts.fields?.find(item => item.name === name)
                                    if (property) onChange({...collection, fields: {...collection.fields, [name]: emptyBinding(schemaAccepts(property))}})
                                }}/>
                    ) : accepts?.fields ? null : (
                        <Div className="blueprint-input__buttons">
                            <Input text={field} placeholder="Field name" onChanged={setField}/>
                            <Button variant="secondary" text="Add" disabled={!field || Object.hasOwn(collection.fields, field)} onClicked={() => {
                                onChange({...collection, fields: {...collection.fields, [field]: literal(null)}})
                                setField('')
                            }}/>
                        </Div>
                    ) : null}
                </>
            ) : null}
        </Div>
    )
}
