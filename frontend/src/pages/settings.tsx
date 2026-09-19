import {useState} from 'react'
import {useNavigate} from 'react-router-dom'
import '../styles/settings.scss'
import {Button, Div} from '../components/primitives'
import {
    ActiveModelField,
    AutoConfirmField,
    NameTimezoneFields,
    NotificationsField,
} from '../components/settings-fields.tsx'
import {ThemePreview} from '../components/theme-preview.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useSettings} from '../context/use-settings.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {getAutoConfirm, setAutoConfirm} from '../utils/auto-confirm.ts'
import {requestNotificationPermission} from '../utils/notifications'
import {validateTimezone} from '../utils/validate-timezone.ts'

const browserTimezoneOffsetHours = (): number => -new Date().getTimezoneOffset() / 60;

const Settings = () => {
    useDocumentTitle('Settings')
    const navigate = useNavigate()
    const {settings, setSettings} = useSettings()
    const [name, setName] = useState(settings?.name ?? '')
    const [timezoneText, setTimezoneText] = useState(String(settings?.timezone ?? browserTimezoneOffsetHours()))
    const [notificationsEnabled, setNotificationsEnabled] = useState(settings?.notifications_enabled ?? false)
    const [autoConfirmEnabled, setAutoConfirmEnabled] = useState(getAutoConfirm)

    const onToggleNotifications = async (enabled: boolean) => {
        if (!enabled) {
            setNotificationsEnabled(false)
            return
        }

        setNotificationsEnabled(await requestNotificationPermission())
    }

    const onToggleAutoConfirm = (enabled: boolean) => {
        setAutoConfirm(enabled)
        setAutoConfirmEnabled(enabled)
    }

    const onBack = () => navigate('/')

    const tz = validateTimezone(timezoneText)
    const nameValid = name.trim().length > 0
    const saveDisabled = !nameValid || !tz.valid

    const onSave = async () => {
        if (saveDisabled) return

        const timezone = Number(timezoneText)
        await setSettings({name: name.trim(), timezone, notifications_enabled: notificationsEnabled})
        navigate('/')
    }

    return (
        <Div className="page center vbox settings">
            <TypewriterLabel className="settings__title" text="[ Settings ]" charIntervalMs={30}/>
            <Div className="dos-frame settings__panel">
                <span className="dos-frame__title">Settings</span>
                <Div className="dos-frame__body settings__body">
                    <NameTimezoneFields name={name} onNameChanged={setName} timezoneText={timezoneText}
                                        onTimezoneChanged={setTimezoneText}/>
                    <ThemePreview/>
                    <ActiveModelField
                        provider={settings?.llm_provider ?? 'ollama'}
                        model={settings?.active_model ?? null}
                        onChosen={(llm_provider, active_model) => setSettings({llm_provider, active_model})}
                    />
                    <NotificationsField enabled={notificationsEnabled} onToggle={onToggleNotifications}/>
                    <AutoConfirmField enabled={autoConfirmEnabled} onToggle={onToggleAutoConfirm}/>
                    <Div className="settings__actions">
                        <Button className="settings__action" variant="secondary" text="Back" onClicked={onBack}/>
                        <Button className="settings__action" text="Save" onClicked={onSave} disabled={saveDisabled}/>
                    </Div>
                </Div>
            </Div>
        </Div>
    )
};

export default Settings
