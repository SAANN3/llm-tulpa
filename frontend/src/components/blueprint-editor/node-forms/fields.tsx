import {useEffect, useState, type ReactNode} from 'react'
import {ChevronDown, ChevronRight} from 'pixelarticons/react'
import type {InputBinding, NodeCatalog, Workflow} from '../../../api/blueprints/types'
import type {PropertyInfo, PropertySchema, PropertyType} from '../../../api/plugins/types'
import {schemaAccepts, type Accepted} from '../../../utils/blueprint-accepts'
import {literalBinding} from '../../../utils/blueprint-schema'
import {InputBindingEditor} from '../input-binding-editor'
import {Button, Checkbox, Div, Input, Select} from '../../primitives'

export const Field = ({label, children}: {label: string; children: ReactNode}) => (
    <label className="blueprint-editor__field">
        <span>{label}</span>
        {children}
    </label>
)

export const Check = ({label, on, onToggle}: {label: string; on: boolean; onToggle: (on: boolean) => void}) => (
    <label className="blueprint-form__check">
        <Checkbox toggled={on} onToggled={onToggle}/>
        <span>{label}</span>
    </label>
)

export const Binding = ({name, input, workflow, catalog, nodeId, accepts, onChange}: {
    name: string
    input: InputBinding
    workflow: Workflow
    catalog: NodeCatalog
    nodeId: string
    accepts?: Accepted
    onChange: (input: InputBinding) => void
}) => (
    <InputBindingEditor name={name} input={input} workflow={workflow} catalog={catalog} nodeId={nodeId} accepts={accepts} onChange={onChange}/>
)

const TYPES: PropertyType[] = ['string', 'number', 'integer', 'boolean', 'array', 'object']

export const SchemaEditor = ({schema, onChange, depth = 0}: {schema: PropertySchema; onChange: (schema: PropertySchema) => void; depth?: number}) => {
    const setType = (property_type: string) => {
        const next: PropertySchema = {property_type: property_type as PropertyType}
        if (property_type === 'array') next.items = 'string'
        if (property_type === 'object') next.properties = []
        onChange(next)
    }
    const itemSchema = (items: PropertyType | PropertySchema | undefined): PropertySchema =>
        typeof items === 'string' ? {property_type: items} : items ?? {property_type: 'string'}
    return (
        <Div className="blueprint-form__schema">
            <Select values={TYPES} selected={schema.property_type} onChosen={setType}/>
            {schema.property_type === 'array' && depth < 4 ? (
                <SchemaEditor schema={itemSchema(schema.items)} depth={depth + 1}
                              onChange={items => onChange({...schema, items})}/>
            ) : null}
            {schema.property_type === 'object' && depth < 4 ? (
                <Div className="blueprint-form__keys">
                    {(schema.properties ?? []).map((property, index) => (
                        <Div key={`${property.name}-${index}`} className="blueprint-form__key">
                            <Input text={property.name} placeholder="Field" onChanged={name => {
                                const properties = [...(schema.properties ?? [])]
                                properties[index] = {...property, name}
                                onChange({...schema, properties})
                            }}/>
                            <SchemaEditor schema={property} depth={depth + 1} onChange={next => {
                                const properties = [...(schema.properties ?? [])]
                                properties[index] = {...property, ...next, name: property.name, description: property.description, required: property.required}
                                onChange({...schema, properties})
                            }}/>
                            <Button variant="secondary" text="Remove" onClicked={() => {
                                onChange({...schema, properties: (schema.properties ?? []).filter((_, item) => item !== index)})
                            }}/>
                        </Div>
                    ))}
                    <Button variant="secondary" text="Add field" onClicked={() => onChange({
                        ...schema,
                        properties: [...(schema.properties ?? []), {name: `field${(schema.properties ?? []).length + 1}`, description: '', required: true, property_type: 'string'}],
                    })}/>
                </Div>
            ) : null}
        </Div>
    )
}

export const Details = ({text}: {text: string}) => {
    const [open, setOpen] = useState(false)
    return (
        <Div className="blueprint-form__details">
            <Button variant="secondary" onClicked={() => setOpen(value => !value)}>
                {open ? <ChevronDown width={12} height={12}/> : <ChevronRight width={12} height={12}/>}
                <span>Description</span>
            </Button>
            {open ? <small className="blueprint-form__hint">{text}</small> : null}
        </Div>
    )
}

export const ArgumentFields = ({properties, values, schemaKnown, workflow, catalog, nodeId, onChange}: {
    properties: PropertyInfo[]
    values: Record<string, InputBinding>
    schemaKnown: boolean
    workflow: Workflow
    catalog: NodeCatalog
    nodeId: string
    onChange: (values: Record<string, InputBinding>) => void
}) => {
    const known = new Set(properties.map(item => item.name))
    const missing = properties.filter(item => item.required && !Object.hasOwn(values, item.name))
    const optional = properties.filter(item => !item.required && !Object.hasOwn(values, item.name))
    const unknown = Object.keys(values).filter(name => !known.has(name))
    useEffect(() => {
        if (!schemaKnown || unknown.length === 0) return
        const next = {...values}
        for (const name of unknown) delete next[name]
        onChange(next)
    }, [schemaKnown, unknown, values, onChange])
    const set = (name: string, input: InputBinding | null) => {
        const next = {...values}
        if (input) next[name] = input
        else delete next[name]
        onChange(next)
    }
    return (
        <>
            {properties.filter(item => Object.hasOwn(values, item.name)).map(item => (
                <Div key={item.name}>
                    <Binding name={item.name} input={values[item.name]} workflow={workflow} catalog={catalog} nodeId={nodeId}
                             accepts={schemaAccepts(item)} onChange={input => set(item.name, input)}/>
                    {item.description ? <Details text={item.description}/> : null}
                    {!item.required ? <Button variant="secondary" text="Remove argument" onClicked={() => set(item.name, null)}/> : null}
                </Div>
            ))}
            {missing.map(item => (
                <p key={item.name} className="blueprint-editor__warning">{item.name} is required and has no value.</p>
            ))}
            {optional.map(item => (
                <Button key={item.name} variant="secondary" text={`Add ${item.name}`}
                        onClicked={() => set(item.name, literalBinding(item))}/>
            ))}
            {!schemaKnown && unknown.length > 0 ? (
                <p className="blueprint-editor__warning">Saved arguments stay hidden until this definition is available again.</p>
            ) : null}
        </>
    )
}