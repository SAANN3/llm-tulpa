import type {PropertyInfo, PropertySchema} from '../plugins/types'

export type BlueprintSort = 'created' | 'updated'

export type Schedule =
    | {type: 'manual'}
    | {type: 'interval'; seconds: number}
    | {type: 'daily'; time: string; timezone: string}

export interface Blueprint {
    id: number
    user_id: number
    name: string
    revision: number
    schedule: Schedule
    next_due_at: string | null
    created_at: string
    updated_at: string
}

export interface BlueprintList {
    items: Blueprint[]
    total: number
    offset: number
    limit: number
}

export type Literal = null | boolean | number | string | Literal[] | {[key: string]: Literal}
export type InputBinding =
    | {source: 'literal'; value: Literal}
    | {source: 'variable'; node_id: string; path?: (string | number)[]}
    | {source: 'output'; node_id: string; port: string; path?: (string | number)[]}
    | {source: 'array'; items: InputBinding[]}
    | {source: 'object'; fields: Record<string, InputBinding>}

export type ModelChoice = {type: 'user_default'} | {type: 'model'; id: number; launch_profile_id?: number | null}
export type Comparison = 'equal' | 'not_equal' | 'greater' | 'greater_or_equal' | 'less' | 'less_or_equal' | 'contains'
export type Operation =
    | {type: 'add' | 'subtract' | 'multiply' | 'divide' | 'remainder' | 'and' | 'or' | 'concat'; left: InputBinding; right: InputBinding}
    | {type: 'not' | 'length'; value: InputBinding}
    | {type: 'get'; list: InputBinding; index: InputBinding}
    | {type: 'append'; list: InputBinding; value: InputBinding}

export type NodeKind =
    | {type: 'entry'}
    | {type: 'variable'; value_type: PropertySchema; initial: Literal; scope?: 'run' | 'blueprint'}
    | {type: 'set_variable'; variable_id: string; value: InputBinding}
    | {type: 'calculate'; operation: Operation}
    | {type: 'merge'; value_type: PropertySchema; inputs: Record<string, InputBinding>}
    | {type: 'generate'; model: ModelChoice; prompt: InputBinding; think?: boolean | string | null}
    | {type: 'agent'; model: ModelChoice; prompt: InputBinding; system_prompt?: InputBinding | null; tools: {mode: 'blacklist' | 'whitelist'; names: string[]}; think?: boolean | string | null; reuse_chat?: boolean; write_notes?: boolean; write_memory?: boolean}
    | {type: 'tool'; name: string; arguments: Record<string, InputBinding>; require_confirmation?: boolean}
    | {type: 'sleep'; seconds: InputBinding}
    | {type: 'if'; left: InputBinding; operator: Comparison; right: InputBinding}
    | {type: 'while'; left: InputBinding; operator: Comparison; right: InputBinding; max_iterations?: number}
    | {type: 'for_each'; items: InputBinding; max_iterations?: number}
    | {type: 'switch'; value: InputBinding; cases: {port: string; value: Literal}[]}
    | {type: 'format'; template: string; arguments: Record<string, InputBinding>}
    | {type: 'return'; values: Record<string, InputBinding>}
    | {type: 'plugin'; plugin_name: string; plugin_subname: string; node_type: string; version: number; arguments: Record<string, InputBinding>}

export interface WorkflowNode {
    id: string
    name: string
    position: {x: number; y: number}
    kind: NodeKind
}
export interface ControlEdge {id: string; from: string; port: string; to: string}
export interface Workflow {schema_version: number; nodes: WorkflowNode[]; edges: ControlEdge[]}
export interface BlueprintDocument {blueprint: Blueprint; workflow: Workflow}
export interface OutputPort {name: string; value_type: 'null' | 'boolean' | 'integer' | 'number' | 'string' | 'array' | 'object' | 'dynamic'}
export interface NodeDescriptor {
    kind: NodeKind['type']
    title: string
    control_outputs: string[]
    outputs: OutputPort[]
    dynamic_control_outputs: boolean
}
export interface PluginNodeDefinition {
    plugin_name: string
    plugin_subname: string
    node_type: string
    version: number
    title: string
    description: string
    inputs: PropertyInfo[]
    outputs: PropertyInfo[]
    control_outputs: string[]
    timeout_s: number
}
export interface NodeCatalog {nodes: NodeDescriptor[]; plugin_nodes: PluginNodeDefinition[]}
export interface ValidationDiagnostic {severity: 'error' | 'warning'; node_id: string | null; field: string; code: string; message: string}
export interface ValidationReport {valid: boolean; diagnostics: ValidationDiagnostic[]}
