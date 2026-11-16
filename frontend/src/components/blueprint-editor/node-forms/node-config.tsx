import type {AvailableTool} from '../../../api/chats/available-tools'
import type {LaunchProfile} from '../../../api/profiles/types'
import type {ManagedModel} from '../../../api/runtime/types'
import type {ControlEdge, NodeCatalog, NodeKind, Workflow, WorkflowNode} from '../../../api/blueprints/types'
import {CalculateForm, MergeForm, SetVariableForm, SleepForm, VariableForm} from './data-form'
import {AgentForm, GenerateForm} from './model-form'
import {ForEachForm, FormatForm, IfForm, ReturnForm, SwitchForm, WhileForm} from './flow-form'
import {PluginForm} from './plugin-form'
import {ToolForm} from './tool-form'

export const NodeConfig = ({node, workflow, catalog, tools, toolsLoaded, models, profiles, onChange}: {
    node: WorkflowNode
    workflow: Workflow
    catalog: NodeCatalog
    tools: AvailableTool[]
    toolsLoaded: boolean
    models: ManagedModel[]
    profiles: LaunchProfile[]
    onChange: (kind: NodeKind, edges?: ControlEdge[]) => void
}) => {
    const kind = node.kind
    switch (kind.type) {
        case 'entry': return null
        case 'tool': return <ToolForm node={{...node, kind}} workflow={workflow} catalog={catalog} tools={tools} toolsLoaded={toolsLoaded} onChange={onChange}/>
        case 'generate': return <GenerateForm node={{...node, kind}} workflow={workflow} catalog={catalog} models={models} profiles={profiles} onChange={onChange}/>
        case 'agent': return <AgentForm node={{...node, kind}} workflow={workflow} catalog={catalog} models={models} profiles={profiles} tools={tools} onChange={onChange}/>
        case 'variable': return <VariableForm node={{...node, kind}} onChange={onChange}/>
        case 'set_variable': return <SetVariableForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'calculate': return <CalculateForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'merge': return <MergeForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'sleep': return <SleepForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'if': return <IfForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'while': return <WhileForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'for_each': return <ForEachForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'switch': return <SwitchForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'format': return <FormatForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'return': return <ReturnForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
        case 'plugin': return <PluginForm node={{...node, kind}} workflow={workflow} catalog={catalog} onChange={onChange}/>
    }
}
