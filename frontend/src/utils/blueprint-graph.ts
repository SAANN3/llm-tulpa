import type {ControlEdge, InputBinding, Literal, NodeDescriptor, NodeKind, PluginNodeDefinition, Workflow, WorkflowNode} from '../api/blueprints/types'
import type {PropertySchema} from '../api/plugins/types'
import {inputBindings, references, type DataLink, type DataReference} from './blueprint-node-view'

export interface GraphPoint {x: number; y: number}
export interface GraphViewport {
    fit: () => void
    place: (point?: GraphPoint) => GraphPoint | null
}
export type NodeChoice = {type: 'builtin'; definition: NodeDescriptor} | {type: 'plugin'; definition: PluginNodeDefinition}

export const graphId = (prefix: string): string =>
    `${prefix}-${Array.from(crypto.getRandomValues(new Uint32Array(4)), part => part.toString(16)).join('')}`

export const literal = (value: Literal): InputBinding => ({source: 'literal', value})

const initialValue = (schema: PropertySchema): Literal => {
    switch (schema.property_type) {
        case 'string': return ''
        case 'integer': case 'number': return 0
        case 'boolean': return false
        case 'array': return []
        case 'object': return Object.fromEntries((schema.properties ?? []).filter(item => item.required).map(item => [item.name, initialValue(item)]))
    }
}

const defaultKind = (type: NodeDescriptor['kind'], workflow: Workflow): NodeKind => {
    switch (type) {
        case 'entry': return {type}
        case 'variable': return {type, value_type: {property_type: 'string'}, initial: '', scope: 'run'}
        case 'set_variable': return {type, variable_id: workflow.nodes.find(node => node.kind.type === 'variable')?.id ?? 'unconfigured', value: literal(null)}
        case 'calculate': return {type, operation: {type: 'add', left: literal(0), right: literal(0)}}
        case 'merge': return {type, value_type: {property_type: 'string'}, inputs: {}}
        case 'generate': return {type, model: {type: 'user_default'}, prompt: literal('')}
        case 'agent': return {type, model: {type: 'user_default'}, prompt: literal(''), tools: {mode: 'blacklist', names: []}}
        case 'tool': return {type, name: 'unconfigured', arguments: {}}
        case 'sleep': return {type, seconds: literal(1)}
        case 'if': return {type, left: literal(0), operator: 'equal', right: literal(0)}
        case 'while': return {type, left: literal(0), operator: 'less', right: literal(0), max_iterations: 1000}
        case 'for_each': return {type, items: literal([]), max_iterations: 1000}
        case 'switch': return {type, value: literal(''), cases: []}
        case 'format': return {type, template: '', arguments: {}}
        case 'return': return {type, values: {}}
        case 'plugin': throw new Error('Plugin nodes require a registered definition.')
    }
}

export const createGraphNode = (choice: NodeChoice, position: GraphPoint, workflow: Workflow): WorkflowNode => {
    const definition = choice.definition
    const kind: NodeKind = choice.type === 'builtin' ? defaultKind(choice.definition.kind, workflow) : {
        type: 'plugin', plugin_name: choice.definition.plugin_name, plugin_subname: choice.definition.plugin_subname,
        node_type: choice.definition.node_type, version: choice.definition.version,
        arguments: Object.fromEntries(choice.definition.inputs.filter(input => input.required).map(input => [input.name, literal(initialValue(input))])),
    }
    return {id: graphId('node'), name: definition.title, position, kind}
}

export const withNodeInput = (node: WorkflowNode, name: string, input: InputBinding): WorkflowNode => {
    if (!inputBindings(node).some(([key]) => key === name)) return node
    const kind = node.kind
    switch (kind.type) {
        case 'entry': case 'variable': return node
        case 'tool': case 'plugin': case 'format': return {...node, kind: {...kind, arguments: {...kind.arguments, [name]: input}}}
        case 'merge': return {...node, kind: {...kind, inputs: {...kind.inputs, [name]: input}}}
        case 'return': return {...node, kind: {...kind, values: {...kind.values, [name]: input}}}
        case 'calculate': return {...node, kind: {...kind, operation: {...kind.operation, [name]: input}}}
        default: return {...node, kind: {...kind, [name]: input}}
    }
}

const withoutReference = (input: InputBinding, id: string): InputBinding => {
    switch (input.source) {
        case 'output': case 'variable': return input.node_id === id ? literal(null) : input
        case 'array': return {...input, items: input.items.map(item => withoutReference(item, id))}
        case 'object': return {...input, fields: Object.fromEntries(Object.entries(input.fields).map(([key, value]) => [key, withoutReference(value, id)]))}
        case 'literal': return input
    }
}

// Merge bindings use edge ids, so changing a control edge must update its binding in the same undo step.
export const withControlEdges = (workflow: Workflow, edges: ControlEdge[]): Workflow => ({
    ...workflow, edges,
    nodes: workflow.nodes.map(node => {
        if (node.kind.type !== 'merge') return node
        const before = workflow.edges.filter(edge => edge.to === node.id).map(edge => edge.id)
        const after = edges.filter(edge => edge.to === node.id).map(edge => edge.id)
        const inputs = {...node.kind.inputs}
        for (const id of before) if (!after.includes(id)) delete inputs[id]
        for (const id of after) if (!Object.hasOwn(inputs, id)) inputs[id] = literal(null)
        return {...node, kind: {...node.kind, inputs}}
    }),
})

export const removeGraphNode = (workflow: Workflow, id: string): Workflow => {
    const nodes = workflow.nodes.filter(node => node.id !== id).map(node => {
        let next = node
        for (const [name, input] of inputBindings(node)) next = withNodeInput(next, name, withoutReference(input, id))
        if (next.kind.type === 'set_variable' && next.kind.variable_id === id) {
            next = {...next, kind: {...next.kind, variable_id: 'unconfigured'}}
        }
        return next
    })
    return withControlEdges({...workflow, nodes}, workflow.edges.filter(edge => edge.from !== id && edge.to !== id))
}

export const controlConnectionError = (workflow: Workflow, from: string, port: string, to: string, replacing?: string): string | null => {
    const target = workflow.nodes.find(node => node.id === to)
    if (!target || !workflow.nodes.some(node => node.id === from)) return 'A node is no longer available.'
    if (from === to) return 'A node cannot connect to itself.'
    if (target.kind.type === 'entry') return 'Start has no control input.'
    const edges = workflow.edges.filter(edge => edge.id !== replacing)
    if (edges.some(edge => edge.from === from && edge.port === port && edge.to === to)) return 'This connection already exists.'
    if (target.kind.type !== 'merge' && edges.some(edge => edge.to === to)) return 'This control input is occupied. Use a Merge node for multiple incoming control edges.'
    const visited = new Set<string>()
    const pending = [to]
    while (pending.length) {
        const id = pending.pop()!
        if (id === from) return 'Use While or For each instead of a control back edge.'
        if (visited.has(id)) continue
        visited.add(id)
        pending.push(...edges.filter(edge => edge.from === id).map(edge => edge.to))
    }
    return null
}

export const nodeReferences = (workflow: Workflow, id: string): number => workflow.nodes.reduce((total, node) =>
    total + inputBindings(node).filter(([, input]) => references(input).some(ref => ref.node === id)).length
        + (node.kind.type === 'set_variable' && node.kind.variable_id === id ? 1 : 0), 0)


export const dataReferenceAt = (workflow: Workflow, link: DataLink): DataReference | null => {
    const node = workflow.nodes.find(item => item.id === link.node)
    let input = node && inputBindings(node).find(([name]) => name === link.input)?.[1]
    for (const part of link.bindingPath) {
        input = input?.source === 'array' && typeof part === 'number' ? input.items[part]
            : input?.source === 'object' && typeof part === 'string' ? input.fields[part] : undefined
    }
    return input?.source === 'output' || input?.source === 'variable' ? input : null
}

export const replaceDataReference = (workflow: Workflow, link: DataLink, value: InputBinding): Workflow => {
    const replace = (input: InputBinding, depth: number): InputBinding => {
        if (depth === link.bindingPath.length) return value
        const part = link.bindingPath[depth]
        if (input.source === 'array' && typeof part === 'number') {
            return {...input, items: input.items.map((item, index) => index === part ? replace(item, depth + 1) : item)}
        }
        if (input.source === 'object' && typeof part === 'string' && Object.hasOwn(input.fields, part)) {
            return {...input, fields: {...input.fields, [part]: replace(input.fields[part], depth + 1)}}
        }
        return input
    }
    return {
        ...workflow,
        nodes: workflow.nodes.map(node => {
            if (node.id !== link.node) return node
            const input = inputBindings(node).find(([name]) => name === link.input)?.[1]
            return input ? withNodeInput(node, link.input, replace(input, 0)) : node
        }),
    }
}

export const dataConnectionError = (workflow: Workflow, from: string, to: string, name: string): string | null => {
    const target = workflow.nodes.find(node => node.id === to)
    if (!workflow.nodes.some(node => node.id === from) || !target) return 'A node is no longer available.'
    if (!inputBindings(target).some(([key]) => key === name)) return 'This data input is no longer available.'
    if (from === to) return 'A node cannot read its own output.'
    return null
}
