import {useState} from 'react'
import type {BlueprintDocument} from '../../api/blueprints/types'
import {useBlueprintVariables} from '../../hooks/blueprint-editor/use-blueprint-variables'
import {ConfirmPopup} from '../popups/base/confirm-popup'
import {Button, Div, Label} from '../primitives'

type Reset = {revision: number; variable?: {id: string; name: string}}

export const SavedVariables = ({saved, locked}: {saved: BlueprintDocument; locked: boolean}) => {
    const {id, revision} = saved.blueprint
    const {values, loading, readError, resetError, resetting, reload, reset} = useBlueprintVariables(id, revision)
    const [pending, setPending] = useState<Reset | null>(null)
    const declarations = new Map(saved.workflow.nodes
        .filter(node => node.kind.type === 'variable' && node.kind.scope === 'blueprint')
        .map(node => [node.id, node.name]))
    const entries = Object.entries(values ?? {})
    const disabled = locked || loading || resetting || readError !== null
    const confirmation = pending?.revision === revision && !locked ? pending : null
    return (
        <section className="blueprint-editor__saved-variables">
            <Label className="section-heading" text="Saved variables"/>
            <Label variant="secondary" text="Values kept between runs. After a reset, the next run uses the variable's initial value."/>
            {readError || resetError ? <Label text={readError ?? resetError ?? ''}/> : null}
            {values === null && loading ? <Label text="Loading saved variables…"/> : null}
            {values !== null && entries.length === 0 ? <Label variant="secondary" text="No saved variables."/> : null}
            {entries.map(([nodeId, value]) => {
                const name = declarations.get(nodeId)
                return (
                    <Div key={nodeId} className="blueprint-editor__run-row">
                        <Label text={name ?? nodeId}/>
                        <Label variant="secondary" text={name === undefined ? 'No saved blueprint-scoped declaration' : nodeId}/>
                        <pre className="blueprint-editor__saved-value">{JSON.stringify(value, null, 2)}</pre>
                        <Div className="blueprint-editor__run-actions">
                            <Button variant="secondary" text="Reset" disabled={disabled}
                                    onClicked={() => setPending({revision, variable: {id: nodeId, name: name ?? nodeId}})}/>
                        </Div>
                    </Div>
                )
            })}
            <Div className="blueprint-editor__run-actions">
                <Button variant="secondary" text="Reload" disabled={loading || resetting} onClicked={() => { void reload() }}/>
                <Button variant="secondary" text="Reset all" disabled={disabled || entries.length === 0}
                        onClicked={() => setPending({revision})}/>
            </Div>
            <ConfirmPopup open={confirmation !== null} title={confirmation?.variable ? 'Reset saved variable' : 'Reset all saved variables'}
                          confirmLabel={confirmation?.variable ? 'Reset' : 'Reset all'}
                          message={confirmation?.variable
                              ? `Remove the saved value of "${confirmation.variable.name}"?`
                              : 'Remove every saved variable value for this blueprint?'}
                          onClose={() => setPending(null)}
                          onConfirm={() => { if (confirmation) void reset(confirmation.variable?.id) }}/>
        </section>
    )
}
