import type {InputBinding, NodeCatalog, Operation, Workflow, WorkflowNode} from '../api/blueprints/types'
import type {PropertyInfo, PropertySchema, PropertyType} from '../api/plugins/types'
import {literal} from './blueprint-graph'
import {nodeView} from './blueprint-node-view'

export type ValueKind = 'null' | 'boolean' | 'integer' | 'number' | 'string' | 'array' | 'object' | 'dynamic'

/** Types a binding may have. An empty `types` list means the consumer accepts anything. */
export interface Accepted {
    types: Array<Exclude<ValueKind, 'dynamic'>>
    items?: Accepted
    fields?: PropertyInfo[]
}

export const TEXT: Accepted = {types: ['string']}
export const NUMBER: Accepted = {types: ['number', 'integer']}
export const ARRAY: Accepted = {types: ['array']}

const CHOICE_ORDER = ['Text', 'Number', 'Boolean', 'Null', 'Array', 'Object'] as const

const schemaOf = (items: PropertyType | PropertySchema): PropertySchema =>
    typeof items === 'string' ? {property_type: items} : items

/** An object schema with no fields is an open value: validation treats it as dynamic. */
const declaredKind = (schema: PropertySchema): ValueKind =>
    schema.property_type === 'object' && !(schema.properties ?? []).length ? 'dynamic' : schema.property_type

export const schemaAccepts = (schema: PropertySchema): Accepted => ({
    types: [schema.property_type === 'object' && !(schema.properties ?? []).length ? 'object' : schema.property_type],
    items: schema.property_type === 'array' && schema.items ? schemaAccepts(schemaOf(schema.items)) : undefined,
    fields: schema.property_type === 'object' && schema.properties?.length ? schema.properties : undefined,
})

export const comparisonAccepts = (operator: string): Accepted | undefined => {
    if (operator === 'equal' || operator === 'not_equal') return undefined
    if (operator === 'contains') return TEXT
    return NUMBER
}

export const operandAccepts = (operation: Operation, side: 'left' | 'right' | 'list' | 'index' | 'value'): Accepted | undefined => {
    switch (operation.type) {
        case 'add': case 'subtract': case 'multiply': case 'divide': case 'remainder': return NUMBER
        case 'and': case 'or': case 'not': return {types: ['boolean']}
        case 'length': return {types: ['string', 'array']}
        case 'get': return side === 'index' ? {types: ['integer']} : ARRAY
        case 'append': return side === 'list' ? ARRAY : undefined
        case 'concat': return ARRAY
    }
}

const operationKind = (operation: Operation): ValueKind => {
    switch (operation.type) {
        case 'divide': return 'number'
        case 'add': case 'subtract': case 'multiply': case 'remainder': return 'number'
        case 'and': case 'or': case 'not': return 'boolean'
        case 'length': return 'integer'
        case 'append': case 'concat': return 'array'
        default: return 'dynamic'
    }
}

export const portInfo = (node: WorkflowNode, port: string, workflow: Workflow, catalog: NodeCatalog): {kind: ValueKind; schema?: PropertySchema} => {
    const kind = node.kind
    if (kind.type === 'variable' && port === 'value') return {kind: declaredKind(kind.value_type), schema: kind.value_type}
    if (kind.type === 'merge' && port === 'value') return {kind: declaredKind(kind.value_type), schema: kind.value_type}
    if (kind.type === 'set_variable' && port === 'value') {
        const variable = workflow.nodes.find(item => item.id === kind.variable_id && item.kind.type === 'variable')
        return variable?.kind.type === 'variable' ? {kind: declaredKind(variable.kind.value_type), schema: variable.kind.value_type} : {kind: 'dynamic'}
    }
    if (kind.type === 'calculate' && port === 'value') return {kind: operationKind(kind.operation)}
    if (kind.type === 'plugin') {
        const registration = catalog.plugin_nodes.find(item => item.plugin_name === kind.plugin_name && item.plugin_subname === kind.plugin_subname && item.node_type === kind.node_type && item.version === kind.version)
        const output = registration?.outputs.find(item => item.name === port)
        return output ? {kind: declaredKind(output), schema: output} : {kind: 'dynamic'}
    }
    const declared = catalog.nodes.find(item => item.kind === kind.type)?.outputs.find(item => item.name === port)?.value_type
    return {kind: declared ?? 'dynamic'}
}

const scalarFits = (actual: ValueKind, types: Accepted['types']): boolean => {
    if (actual === 'integer') return types.includes('integer') || types.includes('number')
    return actual !== 'dynamic' && actual !== 'array' && actual !== 'object' && types.includes(actual)
}

/** A value fits when it is the asked type, dynamic, or a container that can be pathed down to one. */
export const fits = (actual: ValueKind, schema: PropertySchema | undefined, accepted?: Accepted): boolean => {
    if (!accepted || accepted.types.length === 0) return true
    if (actual === 'dynamic' || scalarFits(actual, accepted.types)) return true
    if (actual === 'array') {
        if (!schema?.items) return true
        const item = schemaOf(schema.items)
        return fits(declaredKind(item), item, accepted)
    }
    if (actual === 'object') {
        const fields = schema?.properties ?? []
        return fields.length === 0 || fields.some(field => fits(declaredKind(field), field, accepted))
    }
    return false
}

export const literalChoices = (accepted?: Accepted): string[] => {
    if (!accepted || accepted.types.length === 0) return [...CHOICE_ORDER]
    const names = new Set<string>()
    for (const type of accepted.types) {
        names.add(type === 'string' ? 'Text' : type === 'number' || type === 'integer' ? 'Number' : type === 'boolean' ? 'Boolean' : type === 'null' ? 'Null' : type === 'array' ? 'Array' : 'Object')
    }
    return CHOICE_ORDER.filter(choice => names.has(choice))
}

export const emptyBinding = (accepted?: Accepted): InputBinding => {
    const types = accepted?.types ?? []
    const type = types.length === 1 ? types[0] : undefined
    switch (type) {
        case 'string': return literal('')
        case 'number': case 'integer': return literal(0)
        case 'boolean': return literal(false)
        case 'null': return literal(null)
        case 'array': return {source: 'array', items: []}
        case 'object': return {source: 'object', fields: {}}
        default: return literal(null)
    }
}

export const integerOnly = (accepted?: Accepted): boolean =>
    !!accepted && accepted.types.includes('integer') && !accepted.types.includes('number') && !accepted.types.includes('string')

export const compatiblePorts = (node: WorkflowNode, workflow: Workflow, catalog: NodeCatalog, accepted?: Accepted): string[] =>
    nodeView(node, catalog).outputs.map(port => port.name).filter(port => {
        const info = portInfo(node, port, workflow, catalog)
        return fits(info.kind, info.schema, accepted)
    })
