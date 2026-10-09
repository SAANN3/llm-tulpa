import {useState} from 'react'
import {useNavigate} from 'react-router-dom'
import '../styles/settings.scss'
import {Frame} from '../components/frame.tsx'
import {Button, Div, Label} from '../components/primitives'
import {
    ActiveModelField,
    AutoConfirmField,
    TrimOldThinkingField,
    DebugField,
    UseToolsField,
    MaxTurnStepsField,
    NameTimezoneFields,
    NotificationsField,
    TimeFormatField,
    HfTokenField,
} from '../components/settings-fields.tsx'
import {BackgroundPicker} from '../components/background-picker.tsx'
import {ThemePreview} from '../components/theme-preview.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useSettings} from '../context/use-settings.ts'
import {useDebug} from '../hooks/use-debug.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {useGoBack} from '../hooks/use-go-back.ts'
import {setDebug} from '../utils/debug.ts'
import {requestNotificationPermission} from '../utils/notifications'
import {parseMaxTurnSteps} from '../utils/parse-max-turn-steps.ts'
import {validateTimezone} from '../utils/validate-timezone.ts'

const browserTimezoneOffsetHours = (): number => -new Date().getTimezoneOffset() / 60;

const Settings = () => {
    useDocumentTitle('Settings')
    const navigate = useNavigate()
    const {settings, setSettings} = useSettings()
    const [name, setName] = useState(settings?.name ?? '')
    const [timezoneText, setTimezoneText] = useState(String(settings?.timezone ?? browserTimezoneOffsetHours()))
    const [notificationsEnabled, setNotificationsEnabled] = useState(settings?.notifications_enabled ?? false)
    const [autoConfirmEnabled, setAutoConfirmEnabled] = useState(settings?.auto_confirm ?? false)
    const [trimOldThinking, setTrimOldThinking] = useState(settings?.trim_old_thinking ?? false)
    const [useTools, setUseTools] = useState(settings?.use_tools ?? true)
    const debug = useDebug()
    const [maxTurnStepsText, setMaxTurnStepsText] = useState(settings?.max_turn_steps != null ? String(settings.max_turn_steps) : '')

    const onToggleNotifications = async (enabled: boolean) => {
        if (!enabled) {
            setNotificationsEnabled(false)
            return
        }

        setNotificationsEnabled(await requestNotificationPermission())
    }

    const onBack = useGoBack()

    const tz = validateTimezone(timezoneText)
    const nameValid = name.trim().length > 0
    const steps = parseMaxTurnSteps(maxTurnStepsText)
    const saveDisabled = !nameValid || !tz.valid || !steps.valid

    const onSave = async () => {
        if (saveDisabled) return

        const timezone = Number(timezoneText)
        await setSettings({name: name.trim(), timezone, notifications_enabled: notificationsEnabled, auto_confirm: autoConfirmEnabled, trim_old_thinking: trimOldThinking, use_tools: useTools, max_turn_steps: steps.value})
        onBack()
    }

    return (
        <Div className="page center vbox settings">
            <TypewriterLabel className="settings__title" text="[ Settings ]" charIntervalMs={30}/>
            <Frame className="settings__panel" bodyClassName="settings__body" title="Settings"
                   actions={[]} onEscape={onBack} escapeLabel="back">
                <NameTimezoneFields name={name} onNameChanged={setName} timezoneText={timezoneText}
                                    onTimezoneChanged={setTimezoneText}/>
                <ThemePreview/>
                <BackgroundPicker/>
                <TimeFormatField/>
                <Div className="field">
                    <Label className="field__label" text="System prompt"/>
                    <Div className="field__control">
                        <Label variant="secondary" text="How the model behaves in your chats"/>
                        <Button variant="secondary" text="Customize" onClicked={() => navigate('/settings/system-prompt')}/>
                    </Div>
                </Div>
                <ActiveModelField
                    provider={settings?.llm_provider ?? 'ollama'}
                    model={settings?.active_model ?? null}
                    launchProfileId={settings?.launch_profile_id ?? null}
                    onChosen={(llm_provider, active_model, launch_profile_id) => setSettings({llm_provider, active_model, launch_profile_id})}
                />
                <HfTokenField hasToken={settings?.has_hf_token ?? false} onSave={(hf_token) => setSettings({hf_token})}/>
                <NotificationsField enabled={notificationsEnabled} onToggle={onToggleNotifications}/>
                <AutoConfirmField enabled={autoConfirmEnabled} onToggle={setAutoConfirmEnabled}/>
                <TrimOldThinkingField enabled={trimOldThinking} onToggle={setTrimOldThinking}/>
                <UseToolsField enabled={useTools} onToggle={setUseTools}/>
                <MaxTurnStepsField text={maxTurnStepsText} onChanged={setMaxTurnStepsText}/>
                <DebugField enabled={debug} onToggle={setDebug}/>
                <Div className="settings__actions">
                    <Button className="settings__action" variant="secondary" text="Back" onClicked={onBack}/>
                    <Button className="settings__action" text="Save" onClicked={onSave} disabled={saveDisabled}/>
                </Div>
            </Frame>
        </Div>
    )
};

export default Settings
