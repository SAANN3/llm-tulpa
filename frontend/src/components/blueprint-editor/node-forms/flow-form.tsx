import {useState} from 'react'
import type {Comparison, ControlEdge, InputBinding, Literal, NodeCatalog, NodeOf, Workflow, WorkflowNode} from '../../../api/blueprints/types'
import {ARRAY, comparisonAccepts} from '../../../utils/blueprint-accepts'
import {literal} from '../../../utils/blueprint-graph'
import {namedKey} from '../../../utils/blueprint-schema'
import {Binding, Field} from './fields'
import {ConfirmPopup} from '../../popups/base/confirm-popup'
import {Button, Div, Input, Label, Select, TextField} from '../../primitives'

const COMPARISONS: Comparison[] = ['equal', 'not_equal', 'greater', 'greater_or_equal', 'less', 'less_or_equal', 'contains']

const Compare = ({node, workflow, catalog, onChange, iterations}: {
    node: NodeOf<'if' | 'while'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
    iterations?: boolean
}) => {
    const kind = node.kind
    return (
        <>
            <Binding name="Left" input={kind.left} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={comparisonAccepts(kind.operator)} onChange={left => onChange({...kind, left})}/>
            <Field label="Operator">
                <Select values={COMPARISONS} selected={kind.operator} onChosen={operator => onChange({...kind, operator: operator as Comparison})}/>
            </Field>
            <Binding name="Right" input={kind.right} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={comparisonAccepts(kind.operator)} onChange={right => onChange({...kind, right})}/>
            {iterations && kind.type === 'while' ? (
                <Field label="Max iterations">
                    <Input type="number" text={String(kind.max_iterations ?? 1000)} onChanged={text => {
                        const max_iterations = Number(text)
                        if (Number.isInteger(max_iterations) && max_iterations >= 1 && max_iterations <= 10000) onChange({...kind, max_iterations})
                    }}/>
                </Field>
            ) : null}
        </>
    )
}

export const IfForm = (props: Omit<Parameters<typeof Compare>[0], 'iterations'>) => <Compare {...props}/>
export const WhileForm = (props: Omit<Parameters<typeof Compare>[0], 'iterations'>) => <Compare {...props} iterations/>

export const ForEachForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'for_each'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
}) => (
    <>
        <Binding name="Items" input={node.kind.items} workflow={workflow} catalog={catalog} nodeId={node.id} accepts={ARRAY} onChange={items => onChange({...node.kind, items})}/>
        <Field label="Max iterations">
            <Input type="number" text={String(node.kind.max_iterations ?? 1000)} onChanged={text => {
                const max_iterations = Number(text)
                if (Number.isInteger(max_iterations) && max_iterations >= 1 && max_iterations <= 10000) onChange({...node.kind, max_iterations})
            }}/>
        </Field>
    </>
)

const readCase = (text: string): Literal => {
    const trimmed = text.trim()
    if (trimmed === 'null' || trimmed === 'true' || trimmed === 'false' || /^-?\d/.test(trimmed) || trimmed.startsWith('{') || trimmed.startsWith('[')) {
        try { return JSON.parse(trimmed) as Literal } catch { return text }
    }
    return text
}

const showCase = (value: Literal): string => typeof value === 'string' ? value : JSON.stringify(value)

export const SwitchForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'switch'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind'], edges?: ControlEdge[]) => void
}) => {
    const kind = node.kind
    const [pending, setPending] = useState<string | null>(null)
    const [drafts, setDrafts] = useState<Record<number, string>>({})
    const used = (port: string, index: number) => kind.cases.some((item, itemIndex) => itemIndex !== index && item.port === port)
    const rename = (index: number, port: string) => {
        if (!namedKey(port) || port === 'default' || used(port, index)) return
        const previous = kind.cases[index].port
        const cases = kind.cases.map((item, itemIndex) => itemIndex === index ? {...item, port} : item)
        const edges = workflow.edges.map(edge => edge.from === node.id && edge.port === previous ? {...edge, port} : edge)
        onChange({...kind, cases}, edges)
    }
    const remove = (index: number) => {
        const port = kind.cases[index].port
        const edges = workflow.edges.filter(edge => !(edge.from === node.id && edge.port === port))
        onChange({...kind, cases: kind.cases.filter((_, itemIndex) => itemIndex !== index)}, edges)
    }
    const askRemove = (index: number) => {
        const port = kind.cases[index].port
        if (workflow.edges.some(edge => edge.from === node.id && edge.port === port)) setPending(String(index))
        else remove(index)
    }
    return (
        <>
            <Binding name="Value" input={kind.value} workflow={workflow} catalog={catalog} nodeId={node.id} onChange={value => onChange({...kind, value})}/>
            <Label className="section-heading" text="Cases"/>
            {kind.cases.map((item, index) => (
                <Div key={index} className="blueprint-form__key">
                    <Input text={drafts[index] ?? item.port} placeholder="Port" onChanged={port => setDrafts(current => ({...current, [index]: port}))}
                           onBlur={() => {
                               const port = drafts[index]
                               if (port != null) rename(index, port)
                               setDrafts(current => { const next = {...current}; delete next[index]; return next })
                           }}/>
                    <Input text={showCase(item.value)} onChanged={text => onChange({
                        ...kind, cases: kind.cases.map((entry, itemIndex) => itemIndex === index ? {...entry, value: readCase(text)} : entry),
                    })}/>
                    <Button variant="secondary" text="Remove" onClicked={() => askRemove(index)}/>
                    {drafts[index] != null && drafts[index] !== item.port && (!namedKey(drafts[index]) || drafts[index] === 'default' || used(drafts[index], index)) ? (
                        <small className="blueprint-editor__warning">Use a unique name other than default.</small>
                    ) : null}
                </Div>
            ))}
            <Button variant="secondary" text="Add case" onClicked={() => {
                let port = 'case'
                let n = kind.cases.length + 1
                while (!namedKey(port) || port === 'default' || kind.cases.some(item => item.port === port)) port = `case${n++}`
                onChange({...kind, cases: [...kind.cases, {port, value: ''}]})
            }}/>
            <small className="blueprint-form__hint">The default output is used when no case matches. A case value is JSON, or plain text if it is not JSON.</small>
            <ConfirmPopup open={pending != null} title="Remove case" confirmLabel="Remove"
                          message="This case has connections. Removing it removes those connections too."
                          onConfirm={() => { if (pending != null) remove(Number(pending)); setPending(null) }}
                          onClose={() => setPending(null)}/>
        </>
    )
}

const placeholders = (template: string): string[] => {
    const names: string[] = []
    let rest = template
    while (rest.length) {
        const index = rest.search(/[{}]/)
        if (index < 0) break
        rest = rest.slice(index)
        if (rest.startsWith('{{') || rest.startsWith('}}')) rest = rest.slice(2)
        else if (rest.startsWith('{')) {
            const end = rest.indexOf('}')
            if (end < 0) break
            const name = rest.slice(1, end)
            if (namedKey(name)) names.push(name)
            rest = rest.slice(end + 1)
        } else break
    }
    return names
}

const RecordForm = ({title, template, values, workflow, catalog, nodeId, onValues, onTemplate}: {
    title: string
    template?: string
    values: Record<string, InputBinding>
    workflow: Workflow
    catalog: NodeCatalog
    nodeId: string
    onValues: (values: Record<string, InputBinding>) => void
    onTemplate?: (template: string) => void
}) => {
    const [key, setKey] = useState('')
    const named = template == null ? [] : placeholders(template)
    const missing = named.filter(name => !Object.hasOwn(values, name))
    const add = () => {
        if (!namedKey(key) || Object.hasOwn(values, key)) return
        onValues({...values, [key]: literal(null)})
        setKey('')
    }
    return (
        <>
            {onTemplate ? (
                <Field label="Template">
                    <TextField text={template ?? ''} onChanged={onTemplate}/>
                </Field>
            ) : null}
            <Label className="section-heading" text={title}/>
            {Object.entries(values).map(([name, input]) => (
                <Div key={name}>
                    <Binding name={name} input={input} workflow={workflow} catalog={catalog} nodeId={nodeId}
                             onChange={next => onValues({...values, [name]: next})}/>
                    <Button variant="secondary" text="Remove" onClicked={() => {
                        const next = {...values}
                        delete next[name]
                        onValues(next)
                    }}/>
                </Div>
            ))}
            {missing.map(name => <p key={name} className="blueprint-editor__warning">{`{${name}}`} has no value.</p>)}
            <Div className="blueprint-form__key">
                <Input text={key} placeholder="Name" onChanged={setKey} onKeyDown={event => { if (event.key === 'Enter') add() }}/>
                <Button variant="secondary" text="Add" disabled={!namedKey(key) || Object.hasOwn(values, key)} onClicked={add}/>
            </Div>
            {onTemplate ? <small className="blueprint-form__hint">{'Names in {braces}. A brace itself is {{ or }}.'}</small> : null}
        </>
    )
}

export const FormatForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'format'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
}) => (
    <RecordForm title="Arguments" template={node.kind.template} values={node.kind.arguments} workflow={workflow} catalog={catalog} nodeId={node.id}
                onTemplate={template => onChange({...node.kind, template})} onValues={arguments_ => onChange({...node.kind, arguments: arguments_})}/>
)

export const ReturnForm = ({node, workflow, catalog, onChange}: {
    node: NodeOf<'return'>
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (kind: WorkflowNode['kind']) => void
}) => (
    <RecordForm title="Values" values={node.kind.values} workflow={workflow} catalog={catalog} nodeId={node.id}
                onValues={values => onChange({...node.kind, values})}/>
)
