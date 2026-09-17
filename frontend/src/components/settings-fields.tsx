import '../styles/settings-fields.scss'
import {Div, Input, Label, ToggleSwitch} from './primitives'
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

/** Auto-confirm toggle — a local-to-this-browser preference, not a synced setting */
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
