import {useEffect, useState} from 'react'
import {getThinkingCapability} from '../../../api/llm/thinking-capability'
import type {LaunchProfile} from '../../../api/profiles/types'
import type {ManagedModel} from '../../../api/runtime/types'
import type {InputBinding, ModelChoice, NodeCatalog, NodeOf, Workflow, WorkflowNode} from '../../../api/blueprints/types'
import {ToolChoice} from '../../tool-choice'
import type {AvailableTool} from '../../../api/chats/available-tools'
import {TEXT} from '../../../utils/blueprint-accepts'
import {literal} from '../../../utils/blueprint-graph'
import {Binding, Check, Field} from './fields'
import {Label, Select} from '../../primitives'

const USER = 'User\'s model'

const modelLabel = (model: ManagedModel) => model.display_name || model.file

const ModelFields = ({model, models, profiles, onChange}: {
    model: ModelChoice
    models: ManagedModel[]
    profiles: LaunchProfile[]
    onChange: (model: ModelChoice) => void
}) => {
    const current = model.type === 'model' ? models.find(item => item.id === model.id) : null
    const missing = model.type === 'model' && !current
    const labels = [USER, ...models.map(modelLabel)]
    if (missing) labels.push(`Unavailable model ${model.id}`)
    const selected = model.type === 'user_default' ? USER : current ? modelLabel(current) : labels[labels.length - 1]
    const choose = (label: string) => {
        if (label === USER) onChange({type: 'user_default'})
        else {
            const found = models.find(item => modelLabel(item) === label)
            if (found) onChange({type: 'model', id: found.id, launch_profile_id: null})
        }
    }
    const modelProfiles = current ? profiles.filter(profile => profile.model_id === current.id) : []
    const profileLabels = ['None', ...modelProfiles.map(profile => profile.name)]
    const profile = model.type === 'model' && model.launch_profile_id
        ? modelProfiles.find(item => item.id === model.launch_profile_id) : null
    const profileSelected = profile?.name ?? (model.type === 'model' && model.launch_profile_id ? `Unavailable profile ${model.launch_profile_id}` : 'None')
    if (model.type === 'model' && model.launch_profile_id && !profile) profileLabels.push(profileSelected)
    return (
        <>
            <Field label="Model">
                <Select values={labels} selected={selected} onChosen={choose}/>
            </Field>
            {missing ? <p className="blueprint-editor__warning">This model is not in the registered llama.cpp models. The saved id is kept.</p> : null}
            {model.type === 'model' && (modelProfiles.length > 0 || model.launch_profile_id) ? (
                <Field label="Launch profile">
                    <Select values={profileLabels} selected={profileSelected} onChosen={name => {
                        const found = modelProfiles.find(item => item.name === name)
                        onChange({...model, launch_profile_id: found?.id ?? null})
                    }}/>
                </Field>
            ) : null}
        </>
    )
}

const ThinkField = ({model, models, profiles, think, onChange}: {
    model: ModelChoice
    models: ManagedModel[]
    profiles: LaunchProfile[]
    think: boolean | string | null | undefined
    onChange: (think: boolean | string | null) => void
}) => {
    const [levels, setLevels] = useState<string[]>([])
    useEffect(() => {
        const managed = model.type === 'model' ? models.find(item => item.id === model.id) : null
        const profile = model.type === 'model' && model.launch_profile_id
            ? profiles.find(item => item.id === model.launch_profile_id) : null
        const target = managed ? {model: managed.file, provider: profile?.provider ?? 'llama-cpp'} : null
        let cancel = false
        getThinkingCapability(undefined, target).then(capability => {
            if (!cancel) setLevels(capability.kind === 'graduated' ? capability.modes : [])
        }).catch(() => { if (!cancel) setLevels([]) })
        return () => { cancel = true }
    }, [model, models, profiles])
    const current = think == null ? 'Default' : think === false ? 'Off' : think === true ? 'On' : String(think)
    const values = ['Default', 'Off', 'On', ...levels.filter(level => level !== 'on' && level !== 'off')]
    if (typeof think === 'string' && !values.includes(think)) values.push(think)
    return (
        <Field label="Thinking">
            <Select values={values} selected={values.includes(current) ? current : 'Default'} onChosen={value => {
                onChange(value === 'Default' ? null : value === 'Off' ? false : value === 'On' ? true : value)
            }}/>
        </Field>
    )
}

const Prompt = ({nodeId, prompt, workflow, catalog, onChange}: {
    nodeId: string
    prompt: InputBinding
    workflow: Workflow
    catalog: NodeCatalog
    onChange: (prompt: InputBinding) => void
}) => <Binding name="Prompt" input={prompt} workflow={workflow} catalog={catalog} nodeId={nodeId} accepts={TEXT} onChange={onChange}/>

export const GenerateForm = ({node, workflow, catalog, models, profiles, onChange}: {
    node: NodeOf<'generate'>
    workflow: Workflow
    catalog: NodeCatalog
    models: ManagedModel[]
    profiles: LaunchProfile[]
    onChange: (kind: WorkflowNode['kind']) => void
}) => {
    const kind = node.kind
    return (
        <>
            <ModelFields model={kind.model} models={models} profiles={profiles} onChange={model => onChange({...kind, model})}/>
            <Prompt nodeId={node.id} prompt={kind.prompt} workflow={workflow} catalog={catalog} onChange={prompt => onChange({...kind, prompt})}/>
            <ThinkField model={kind.model} models={models} profiles={profiles} think={kind.think}
                        onChange={think => onChange({...kind, think})}/>
        </>
    )
}

export const AgentForm = ({node, workflow, catalog, models, profiles, tools, onChange}: {
    node: NodeOf<'agent'>
    workflow: Workflow
    catalog: NodeCatalog
    models: ManagedModel[]
    profiles: LaunchProfile[]
    tools: AvailableTool[]
    onChange: (kind: WorkflowNode['kind']) => void
}) => {
    const kind = node.kind
    return (
        <>
            <ModelFields model={kind.model} models={models} profiles={profiles} onChange={model => onChange({...kind, model})}/>
            <Prompt nodeId={node.id} prompt={kind.prompt} workflow={workflow} catalog={catalog} onChange={prompt => onChange({...kind, prompt})}/>
            <Check label="System prompt" on={kind.system_prompt != null}
                   onToggle={on => onChange({...kind, system_prompt: on ? (kind.system_prompt ?? literal('')) : null})}/>
            {kind.system_prompt ? (
                <Binding name="System prompt" input={kind.system_prompt} workflow={workflow} catalog={catalog} nodeId={node.id}
                         accepts={TEXT} onChange={system_prompt => onChange({...kind, system_prompt})}/>
            ) : null}
            <ThinkField model={kind.model} models={models} profiles={profiles} think={kind.think}
                        onChange={think => onChange({...kind, think})}/>
            <Label className="section-heading" text="Tools"/>
            <ToolChoice catalog={tools} mode={kind.tools.mode} names={kind.tools.names} initiallyCollapsed
                        onMode={(mode, names) => onChange({...kind, tools: {mode, names}})}
                        onNames={names => onChange({...kind, tools: {...kind.tools, names}})}/>
            <Check label="Continue the same hidden chat" on={!!kind.reuse_chat} onToggle={reuse_chat => onChange({...kind, reuse_chat})}/>
            <Check label="Allow chat notes" on={!!kind.write_notes} onToggle={write_notes => onChange({...kind, write_notes})}/>
            <Check label="Allow writing global memory" on={!!kind.write_memory} onToggle={write_memory => onChange({...kind, write_memory})}/>
        </>
    )
}
