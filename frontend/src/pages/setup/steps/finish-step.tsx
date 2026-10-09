import {useState} from 'react'
import {useNavigate} from 'react-router-dom'
import {completeSetup} from '../../../api/runtime/setup'
import {Div, Label, ToggleSwitch} from '../../../components/primitives'
import {SettingsRow} from '../../../components/settings-fields.tsx'
import {useAuth} from '../../../context/use-auth.ts'
import {useSetup} from '../../../context/use-setup.ts'
import {requestNotificationPermission} from '../../../utils/notifications'
import type {StepDef, WizardContext} from '../types.ts'

export interface FinishOptions {
    /** What was set up, as name and value, for a last look before leaving */
    summary: [string, string][]
    /** Asks whether to notify when a reply is ready: on a first run, not on an update */
    withNotifications: boolean
}

export const useFinishStep = ({persist}: WizardContext, {summary, withNotifications}: FinishOptions): StepDef => {
    const navigate = useNavigate()
    const {token} = useAuth()
    const {refresh} = useSetup()
    const [notifications, setNotifications] = useState(false)

    return {
        key: 'finish',
        group: 'All set',
        title: 'All set',
        primaryLabel: 'Finish',
        canNext: true,
        onNext: async () => {
            if (withNotifications) await persist({notifications_enabled: notifications})
            // Best effort: a failure only means the update prompt shows again
            if (token) await completeSetup().then(refresh).catch(() => undefined)
            navigate('/', {replace: true})
            return true
        },
        body: (
            <Div className="setup__step">
                <Label className="setup__lead" text={withNotifications ? "You're ready to go." : "You're up to date."}/>
                <Div className="setup__summary">
                    {summary.map(([name, value]) => (
                        <Div key={name} className="setup__summary-row">
                            <Label variant="secondary" className="setup__summary-name" text={name}/>
                            <Label text={value}/>
                        </Div>
                    ))}
                </Div>
                {withNotifications ? (
                    <Div className="setup__rows">
                        <SettingsRow label="Notifications" help="Notify me when a reply is ready. Asks your browser for permission; you can change it later in Settings.">
                            <ToggleSwitch toggled={notifications} onToggled={async (enabled) => {
                                setNotifications(enabled ? await requestNotificationPermission() : false)
                            }}/>
                        </SettingsRow>
                    </Div>
                ) : null}
                {withNotifications ? (
                    <Label variant="secondary" className="field__help" text="Connect Telegram, Discord or VK any time from the Plugins page."/>
                ) : null}
            </Div>
        ),
    }
};
