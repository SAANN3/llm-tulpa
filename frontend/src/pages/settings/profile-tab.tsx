import {NameTimezoneRow, NotificationsField, SettingsSection} from '../../components/settings-fields.tsx'
import {requestNotificationPermission} from '../../utils/notifications'
import type {SettingsDraftProps} from './draft.ts'

/** Who the user is to the model, and how the app reaches them */
export const ProfileTab = ({draft, onDraft}: SettingsDraftProps) => {
    const onToggleNotifications = async (enabled: boolean) => {
        onDraft({notifications: enabled ? await requestNotificationPermission() : false})
    }
    return (
        <SettingsSection title="Profile">
            <NameTimezoneRow name={draft.name} onNameChanged={(name) => onDraft({name})}
                             timezoneText={draft.timezoneText} onTimezoneChanged={(timezoneText) => onDraft({timezoneText})}/>
            <NotificationsField enabled={draft.notifications} onToggle={onToggleNotifications}/>
        </SettingsSection>
    )
};
