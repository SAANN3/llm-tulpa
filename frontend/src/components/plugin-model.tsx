import {useState} from 'react'
import type {PluginModel} from '../api/plugins/types'
import {useModelsData} from '../hooks/use-models-data.ts'
import type {ModelChoice} from '../utils/model-choice.ts'
import {ChooseModelPopup} from './popups/choose-model-popup.tsx'
import {Button, Div, Label} from './primitives'

export interface PluginModelFieldProps {
    model: PluginModel | null
    /** Saves at once; `null` takes the choice away */
    onChosen: (choice: ModelChoice | null) => Promise<void>
}

/** The model a plugin's chats run on: the same chooser as the chat's model button, saved as soon as it is picked */
export const PluginModelField = ({model, onChosen}: PluginModelFieldProps) => {
    const [open, setOpen] = useState(false)
    const [error, setError] = useState<string | null>(null)
    const {profiles} = useModelsData(false)

    const profileName = profiles.find((p) => p.id === model?.launch_profile_id)?.name ?? null
    const shown = model == null ? 'not chosen' : profileName ? `${model.model} · ${profileName}` : model.model

    const choose = async (choice: ModelChoice | null) => {
        setOpen(false)
        setError(null)
        try {
            await onChosen(choice)
        } catch {
            setError('Could not save the model — is it installed?')
        }
    }

    return (
        <Div className="vbox plugin-settings__field">
            <Label className="section-heading plugin-settings__label" text="model"/>
            <Div className="plugin-settings__model">
                <Label className="plugin-settings__model-name" text={shown}/>
                <Button variant="secondary" text="Change" onClicked={() => setOpen(true)}/>
                <Button variant="secondary" text="Clear" disabled={model == null} onClicked={() => void choose(null)}/>
            </Div>
            {error ? <Label className="plugin-settings__error" text={error}/> : null}
            <Label variant="secondary" className="plugin-settings__desc"
                   text="The model this plugin's chats use, from the next message on. Not chosen: each chat keeps the model it has."/>
            <ChooseModelPopup
                open={open}
                provider={model?.provider ?? 'llama-cpp'}
                selected={model?.provider === 'ollama' ? model.model : null}
                selectedProfileId={model?.launch_profile_id ?? null}
                onSelect={(name) => void choose({provider: 'ollama', model: name, profileId: null})}
                onSelectProfile={(profile) => void choose({provider: profile.provider, model: profile.model, profileId: profile.id})}
                onClose={() => setOpen(false)}
            />
        </Div>
    )
};
