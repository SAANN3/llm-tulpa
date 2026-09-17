import {useState} from 'react'
import {useNavigate} from 'react-router-dom'

import '../styles/setup.scss'
import {Button, Div} from '../components/primitives'
import {AutoConfirmField, NameTimezoneFields, NotificationsField} from '../components/settings-fields.tsx'
import {ThemePreview} from '../components/theme-preview.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useSettings} from '../context/use-settings.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {getAutoConfirm, setAutoConfirm} from '../utils/auto-confirm.ts'
import {requestNotificationPermission} from '../utils/notifications'
import {validateTimezone} from '../utils/validate-timezone.ts'

/** The browser's local UTC offset in whole hours, used to prefill the timezone field */
const browserTimezoneOffsetHours = (): number => -new Date().getTimezoneOffset() / 60;

const Setup = () => {
    useDocumentTitle('Setup')
    const navigate = useNavigate()
    const {settings, setSettings} = useSettings()
    const [name, setName] = useState(settings?.name ?? '')
    const [timezoneText, setTimezoneText] = useState(String(settings?.timezone ?? browserTimezoneOffsetHours()))
    const [notificationsEnabled, setNotificationsEnabled] = useState(settings?.notifications_enabled ?? false)
    const [autoConfirmEnabled, setAutoConfirmEnabled] = useState(getAutoConfirm)
    const [step, setStep] = useState(0)

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

    const pages = [
        <NameTimezoneFields
            key="name"
            name={name}
            onNameChanged={setName}
            timezoneText={timezoneText}
            onTimezoneChanged={setTimezoneText}
        />,
        <ThemePreview key="theme"/>,
        <NotificationsField key="notifications" enabled={notificationsEnabled} onToggle={onToggleNotifications}/>,
        <AutoConfirmField key="autoconfirm" enabled={autoConfirmEnabled} onToggle={onToggleAutoConfirm}/>,
    ]
    const isFirstPage = step === 0
    const isLastPage = step === pages.length - 1

    const tz = validateTimezone(timezoneText)
    const nameValid = name.trim().length > 0
    const primaryDisabled = !nameValid || !tz.valid

    const onBack = () => setStep(step - 1)

    const onPrimary = async () => {
        if (primaryDisabled) return

        if (!isLastPage) {
            setStep(step + 1)
            return
        }

        const timezone = Number(timezoneText)
        await setSettings({name: name.trim(), timezone, notifications_enabled: notificationsEnabled})
        navigate('/')
    }

    return (
        <Div className="page center vbox setup">
            <TypewriterLabel className="setup__title" text="[ Setup ]" charIntervalMs={30}/>
            <Div className="dos-frame setup__panel">
                <span className="dos-frame__title">Setup</span>
                <Div className="dos-frame__body setup__body">
                    {pages[step]}
                    <Div className="center setup__nav">
                        {!isFirstPage && <Button variant="secondary" text="Back" onClicked={onBack}/>}
                        <Button text={isLastPage ? 'Save' : 'Continue'} onClicked={onPrimary}
                                disabled={primaryDisabled}/>
                    </Div>
                    <Div className="center setup__dots">
                        {pages.map((_, i) => (
                            <div key={i} className={`setup__dot${i === step ? ' setup__dot--active' : ''}`}/>
                        ))}
                    </Div>
                </Div>
            </Div>
        </Div>
    )
};

export default Setup
