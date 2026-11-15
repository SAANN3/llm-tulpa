import type {InputBinding, NodeCatalog, WorkflowNode} from '../api/blueprints/types'

export const inputBindings = (node: WorkflowNode): [string, InputBinding][] => {
    const kind = node.kind
    switch (kind.type) {
        case 'entry': case 'variable': return []
        case 'set_variable': case 'switch': return [['value', kind.value]]
        case 'generate': return [['prompt', kind.prompt]]
        case 'agent': return [['prompt', kind.prompt], ...(kind.system_prompt ? [['system_prompt', kind.system_prompt] as [string, InputBinding]] : [])]
        case 'tool': case 'plugin': case 'format': return Object.entries(kind.arguments)
        case 'merge': return Object.entries(kind.inputs)
        case 'return': return Object.entries(kind.values)
        case 'sleep': return [['seconds', kind.seconds]]
        case 'if': case 'while': return [['left', kind.left], ['right', kind.right]]
        case 'for_each': return [['items', kind.items]]
        case 'calculate': {
            const operation = kind.operation
            if ('left' in operation) return [['left', operation.left], ['right', operation.right]]
            if ('list' in operation) return [['list', operation.list], ...('index' in operation ? [['index', operation.index] as [string, InputBinding]] : [['value', operation.value] as [string, InputBinding]])]
            return [['value', operation.value]]
        }
    }
}

export const nodeView = (node: WorkflowNode, catalog: NodeCatalog) => {
    const kind = node.kind
    if (kind.type === 'plugin') {
        const registration = catalog.plugin_nodes.find(item => item.plugin_name === kind.plugin_name && item.plugin_subname === kind.plugin_subname && item.node_type === kind.node_type && item.version === kind.version)
        return {title: registration?.title ?? kind.node_type, unavailable: !registration,
            control: registration?.control_outputs ?? [],
            outputs: registration?.outputs.map(port => ({name: port.name, type: port.property_type})) ?? []}
    }
    const definition = catalog.nodes.find(item => item.kind === kind.type)
    return {title: definition?.title ?? kind.type, unavailable: !definition,
        control: [...new Set([...(definition?.control_outputs ?? []), ...(kind.type === 'switch' ? kind.cases.map(item => item.port) : [])])],
        outputs: definition?.outputs.map(port => ({name: port.name, type: port.value_type})) ?? []}
}

export const bindingText = (input: InputBinding): string => {
    switch (input.source) {
        case 'literal': return typeof input.value === 'string' ? input.value : JSON.stringify(input.value)
        case 'output': return `${input.node_id}.${input.port}${input.path?.length ? '.' + input.path.join('.') : ''}`
        case 'variable': return `$ ${input.node_id}${input.path?.length ? '.' + input.path.join('.') : ''}`
        case 'array': return `[${input.items.map(bindingText).join(', ')}]`
        case 'object': return Object.entries(input.fields).map(([name, value]) => `${name}: ${bindingText(value)}`).join(', ')
    }
}

export const references = (input: InputBinding): {node: string; port: string}[] => {
    switch (input.source) {
        case 'output': return [{node: input.node_id, port: input.port}]
        case 'variable': return [{node: input.node_id, port: 'value'}]
        case 'array': return input.items.flatMap(references)
        case 'object': return Object.values(input.fields).flatMap(references)
        case 'literal': return []
    }
}
