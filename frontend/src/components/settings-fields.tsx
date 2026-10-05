import {useState} from 'react'
import '../styles/settings-fields.scss'
import {Button, Div, Input, Label, ToggleSwitch} from './primitives'
import {ChooseModelPopup} from './popups/choose-model-popup.tsx'
import {useModelsData} from '../hooks/use-models-data.ts'
import {validateTimezone} from '../utils/validate-timezone.ts'

/** A field label in the small-caps style used above every input in this panel */
const FieldLabel = ({text}: { text: string }) => <Label className="field__label" text={text}/>;

/** Helper/status text under a field */
const FieldHelp = ({text, accent, wide}: { text: string; accent?: boolean; wide?: boolean }) => {
    const className = ['field__help', accent && 'field__help--accent', wide && 'field__help--wide'].filter(Boolean).join(' ')
    return <Label variant="secondary" className={className} text={text}/>
};

export interface NameTimezoneFieldsProps {
    name: string
    onNameChanged: (name: string) => void
    timezoneText: string
    onTimezoneChanged: (text: string) => void
}

/** Name + timezone fields — Settings step 1 / Setup step 1 */
export const NameTimezoneFields = ({name, onNameChanged, timezoneText, onTimezoneChanged}: NameTimezoneFieldsProps) => {
    const tz = validateTimezone(timezoneText)

    return (
        <Div className="field__group">
            <Div className="field">
                <FieldLabel text="Name"/>
                <Input text={name} onChanged={onNameChanged} placeholder="Enter your name"/>
                <FieldHelp text="This name will be used when talking with the AI."/>
            </Div>
            <Div className="field">
                <FieldLabel text="Timezone"/>
                <Div className="field__control">
                    <Input className="field__input--tz" text={timezoneText} onChanged={onTimezoneChanged}
                           placeholder="UTC offset"/>
                    {tz.echo ? <Label variant="secondary" text={tz.echo}/> : null}
                </Div>
                <FieldHelp text={tz.message} accent={!tz.valid}/>
            </Div>
        </Div>
    )
};

export interface ActiveModelFieldProps {
    provider: string
    model: string | null
    /** The launch profile that was chosen with it, when one was; null means the model's first */
    launchProfileId: number | null
    onChosen: (provider: string, model: string, launchProfileId?: number) => void
}

/** The model new chats start with — an existing chat keeps the model it's bound to */
export const ActiveModelField = ({provider, model, launchProfileId, onChosen}: ActiveModelFieldProps) => {
    const [open, setOpen] = useState(false)
    const {models, profiles} = useModelsData(false)

    // New chats start on the profile that was chosen, else the model's first; Ollama's models have none
    const shownProfileId = provider === 'ollama' ? undefined : launchProfileId ?? models.find((m) => m.file === model)?.profile_ids[0]
    const profileName = profiles.find((p) => p.id === shownProfileId)?.name ?? null

    return (
        <Div className="field">
            <FieldLabel text="Default model"/>
            <Div className="field__control">
                <Label text={model ?? 'none selected'}/>
                <Button variant="secondary" text="Change" onClicked={() => setOpen(true)}/>
            </Div>
            {profileName ? <Label variant="secondary" className="field__help" text={`Launch profile: ${profileName}`}/> : null}
            <FieldHelp text="New chats start with this model. Each chat can be switched from its own header."/>
            <ChooseModelPopup
                open={open}
                provider={provider}
                selected={provider === 'ollama' ? model : null}
                selectedProfileId={shownProfileId ?? null}
                onSelect={(chosen) => {
                    onChosen('ollama', chosen)
                    setOpen(false)
                }}
                onSelectProfile={(profile) => {
                    onChosen(profile.provider, profile.model, profile.id)
                    setOpen(false)
                }}
                onClose={() => setOpen(false)}
            />
        </Div>
    )
};

export interface HfTokenFieldProps {
    hasToken: boolean
    /** Saves the token (empty clears it) */
    onSave: (token: string) => Promise<void>
}

/** The user's Hugging Face token, needed to download gated models; saved on its own, never shown again */
export const HfTokenField = ({hasToken, onSave}: HfTokenFieldProps) => {
    const [token, setToken] = useState('')

    return (
        <Div className="field">
            <FieldLabel text="Hugging Face token"/>
            <Div className="field__control">
                <Input type="password" text={token} onChanged={setToken} placeholder={hasToken ? 'a token is set' : 'hf_…'}/>
                <Button variant="secondary" text={token.trim() ? 'Save' : 'Clear'} disabled={!token.trim() && !hasToken}
                        onClicked={() => onSave(token.trim()).then(() => setToken(''))}/>
            </Div>
            <FieldHelp text="Only needed to download gated models. Create one at huggingface.co/settings/tokens (read access)."/>
        </Div>
    )
};

export interface NotificationsFieldProps {
    enabled: boolean
    onToggle: (enabled: boolean) => void
}

/** Notifications toggle — Settings step 3 / Setup step 3 */
export const NotificationsField = ({enabled, onToggle}: NotificationsFieldProps) => (
    <Div className="field">
        <Div className="field__row">
            <Label className="field__row-label" text="Receive notifications when a message is ready"/>
            <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
        </Div>
        <FieldHelp text="Asks your browser for permission — you can change this later in Settings." wide/>
    </Div>
);

export interface AutoConfirmFieldProps {
    enabled: boolean
    onToggle: (enabled: boolean) => void
}

/** Auto-confirm toggle — saved with the rest of the user's settings */
export const AutoConfirmField = ({enabled, onToggle}: AutoConfirmFieldProps) => (
    <Div className="field">
        <Div className="field__row">
            <Label className="field__row-label" text="Auto-confirm tool permissions"/>
            <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
        </Div>
        <FieldHelp
            text="Skips the confirmation prompt and automatically allows whatever the model asks to do — only turn this on if you trust it to run unsupervised."
            accent={enabled}
            wide
        />
    </Div>
);

export interface TrimOldThinkingFieldProps {
    enabled: boolean
    onToggle: (enabled: boolean) => void
}

/** Trim-old-thinking toggle — saved with the rest of the user's settings */
export const TrimOldThinkingField = ({enabled, onToggle}: TrimOldThinkingFieldProps) => (
    <Div className="field">
        <Div className="field__row">
            <Label className="field__row-label" text="Shorten old thinking in long chats"/>
            <ToggleSwitch toggled={enabled} onToggled={onToggle}/>
        </Div>
        <FieldHelp
            text="When a long chat nears the model's context limit, the model's earlier reasoning is cut to its last lines and the newest reasoning is kept longer. This frees room without summarizing, but the model may re-check things it had already worked out."
            accent={enabled}
            wide
        />
    </Div>
);
