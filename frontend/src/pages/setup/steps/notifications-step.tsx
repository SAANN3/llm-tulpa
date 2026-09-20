import {useState} from 'react'
import {Div, Label, ToggleSwitch} from '../../../components/primitives'
import {requestNotificationPermission} from '../../../utils/notifications'
import type {StepDef, WizardContext} from '../types.ts'

export const useNotificationsStep = ({persist}: WizardContext): StepDef => {
    const [notifications, setNotifications] = useState(false)

    return {
        key: 'notifications',
        title: 'Notifications',
        canNext: true,
        onNext: async () => {
            await persist({notifications_enabled: notifications})
            return true
        },
        body: (
            <Div className="setup__step">
                <Div className="field">
                    <Div className="field__row">
                        <Label className="field__row-label" text="Notify me when a reply is ready"/>
                        <ToggleSwitch toggled={notifications} onToggled={async (enabled) => {
                            setNotifications(enabled ? await requestNotificationPermission() : false)
                        }}/>
                    </Div>
                    <Label variant="secondary" className="field__help field__help--wide"
                           text="Asks your browser for permission — you can change this later in Settings."/>
                </Div>
            </Div>
        ),
    }
};
