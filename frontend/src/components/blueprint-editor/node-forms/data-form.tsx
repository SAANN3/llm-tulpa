import {useState} from 'react'
import type {InputBinding, Literal, NodeCatalog, NodeOf, Operation, Workflow, WorkflowNode} from '../../../api/blueprints/types'
import type {PropertySchema} from '../../../api/plugins/types'
import {NUMBER, operandAccepts, schemaAccepts} from '../../../utils/blueprint-accepts'
import {literal} from '../../../utils/blueprint-graph'
import {emptyLiteral} from '../../../utils/blueprint-schema'
import {Binding, Field, SchemaEditor} from './fields'
import {Input, Label, Select} from '../../primitives'

const OPERATIONS = ['add', 'subtract', 'multiply', 'divide', 'remainder', 'and', 'or', 'concat', 'not', 'length', 'get', 'append'] as const

const primaryOf = (operation: Operation): InputBinding =>
    'left' in operation ? operation.left : 'list' in operation ? operation.list : operation.value

const withOperation = (operation: Operation, type: Operation['type']): Operation => {
    const primary = primaryOf(operation)
    const secondary = 'right' in operation ? operation.right : 'index' in operation ? operation.index : literal(0)
    const appended = operation.type === 'append' ? operation.value : secondary
    switch (type) {
        case 'not': case 'length': return {type, value: 'value' in operation && operation.type !== 'append' ? operation.value : primary}
        case 'get': return {type, list: 'list' in operation ? operation.list : primary, index: 'index' in operation ? operation.index : literal(0)}
        case 'append': return {type, list: 'list' in operation ? operation.list : primary, value: appended}
        default: return {type, left: 'left' in operation ? operation.left : primary, right: 'right' in operation ? operation.right : secondary}
    }
}

const literalMatches = (value: Literal, schema: PropertySchema): boolean => {
    switch (schema.property_type) {
        case 'string': return typeof value === 'string'
        case 'number': return typeof value === 'number'
        case 'integer': return typeof value === 'number' && Number.isInteger(value)
        case 'boolean': return typeof value === 'boolean'
        case 'array': return Array.isArray(value)
        case 'object': return value !== null && typeof value === 'object' && !Array.isArray(value)
    }
}

const InitialValue = ({value, schema, onChange}: {value: Literal; schema: PropertySchema; onChange: (value: Literal) => void}) => {
    const [draft, setDraft] = useState<string | null>(null)
    const shown = draft ?? (typeof value === 'string' && schema.property_type === 'string' ? value : JSON.stringify(value))
    return (
        <Input text={shown} onChanged={text => {
            setDraft(text)
            if (schema.property_type === 'string') onChange(text)
            else {
                try {
                    const next = JSON.parse(text) as Literal
                    if (literalMatches(next, schema)) onChange(next)
                } catch { /* the field keeps the draft until it is a value of this type */ }
            }
        }} onBlur={() => setDraft(null)}/>
    )
}

export const VariableForm = ({node, onChange}: {node: NodeOf<'variable'>; onChange: (kind: WorkflowNode['kind']) => void}) => {
    const kind = node.kind
    const changeType = (value_type: PropertySchema) => {
        const initial = literalMatches(kind.initial, value_type) ? kind.initial : emptyLiteral(value_type)
        onChange({...kind, value_type, initial})
    }
    return (
        <>
            <Field label="Value type"><SchemaEditor schema={kind.value_type} onChange={changeType}/></Field>
            <Field label="Scope">
                <Select values={['run', 'blueprint']} selected={kind.scope ?? 'run'} onChosen={scope => onChange({...kind, scope: scope as 'run' | 'blueprint'})}/>
            </Field>
            <Field label="Initial value">
                <InitialValue key={kind.value_type.property_type} value={kind.initial} schema={kind.value_type} onChange={initial => onChange({...kind, initial})}/>
            </Field>
            <small className="blueprint-form__hint">A blueprint scope keeps the value between runs. Run scope starts from this initial value every time.</small>
        </>
    )
}

export const SetVariableForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'set_variable'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
}) => {
    const kind = node.kind
    const variables = workflow.nodes.filter(item => item.kind.type === 'variable')
    const labels = variables.map(item => `${item.name} (${item.id})`)
    const current = variables.find(item => item.id === kind.variable_id)
    if (!current) labels.unshift(`${kind.variable_id} (unavailable)`)
    return (
        <>
            <Field label="Variable">
                <Select values={labels.length ? labels : ['No variables']} selected={current ? `${current.name} (${current.id})` : labels[0]}
                        onChosen={label => {
                            const id = label.match(/\(([^)]+)\)$/)?.[1]
                            const variable = variables.find(item => item.id === id)
                            if (variable) onChange({...kind, variable_id: variable.id})
                        }}/>
            </Field>
            {!current ? <p className="blueprint-editor__warning">This variable declaration is not in the graph. The reference is kept.</p> : null}
            {current && current.kind.type === 'variable' ? <small className="blueprint-form__hint">{current.kind.value_type.property_type} · {current.kind.scope ?? 'run'}</small> : null}
            <Binding name="Value" input={kind.value} workflow={workflow} catalog={catalog} nodeId={node.id}
                     accepts={current?.kind.type === 'variable' ? schemaAccepts(current.kind.value_type) : undefined}
                     onChange={value => onChange({...kind, value})}/>
        </>
    )
}

export const CalculateForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'calculate'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
}) => {
    const kind = node.kind
    const operation = kind.operation
    const set = (next: Operation) => onChange({...kind, operation: next})
    return (
        <>
            <Field label="Operation">
                <Select values={[...OPERATIONS]} selected={operation.type} onChosen={type => set(withOperation(operation, type as Operation['type']))}/>
            </Field>
            {'left' in operation ? (
                <>
                    <Binding name="Left" input={operation.left} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={operandAccepts(operation, 'left')} onChange={left => set({...operation, left})}/>
                    <Binding name="Right" input={operation.right} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={operandAccepts(operation, 'right')} onChange={right => set({...operation, right})}/>
                </>
            ) : null}
            {'list' in operation ? (
                <>
                    <Binding name="List" input={operation.list} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={operandAccepts(operation, 'list')} onChange={list => set({...operation, list})}/>
                    {'index' in operation ? <Binding name="Index" input={operation.index} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={operandAccepts(operation, 'index')} onChange={index => set({...operation, index})}/> : null}
                    {'value' in operation && operation.type === 'append' ? <Binding name="Value" input={operation.value} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={operandAccepts(operation, 'value')} onChange={value => set({...operation, value})}/> : null}
                </>
            ) : null}
            {!('left' in operation) && !('list' in operation) ? (
                <Binding name="Value" input={operation.value} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={operandAccepts(operation, 'value')} onChange={value => set({...operation, value})}/>
            ) : null}
        </>
    )
}

export const MergeForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'merge'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
}) => {
    const kind = node.kind
    const incoming = workflow.edges.filter(edge => edge.to === node.id)
    return (
        <>
            <Field label="Value type"><SchemaEditor schema={kind.value_type} onChange={value_type => onChange({...kind, value_type})}/></Field>
            <Label className="section-heading" text="Incoming"/>
            {incoming.length === 0 ? <small className="blueprint-form__hint">Each incoming connection gets a value here.</small> : null}
            {incoming.map(edge => {
                const source = workflow.nodes.find(item => item.id === edge.from)
                return (
                    <Binding key={edge.id} name={source?.name ?? edge.id} input={kind.inputs[edge.id] ?? literal(null)}
                             workflow={workflow} catalog={catalog} nodeId={node.id} accepts={schemaAccepts(kind.value_type)}
                             onChange={input => onChange({...kind, inputs: {...kind.inputs, [edge.id]: input}})}/>
                )
            })}
        </>
    )
}

export const SleepForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'sleep'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
}) => (
    <Binding name="Seconds" input={node.kind.seconds} workflow={workflow} catalog={catalog} nodeId={node.id}
             accepts={NUMBER} onChange={seconds => onChange({...node.kind, seconds})}/>
)
