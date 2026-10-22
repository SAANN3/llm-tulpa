import {MemoryList} from '../../components/memory-list.tsx'
import {MemoryEnabledField, NameTimezoneRow, NotificationsField, SettingsSection, SliderField} from '../../components/settings-fields.tsx'
import {requestNotificationPermission} from '../../utils/notifications'
import type {SettingsDraftProps} from './draft.ts'

/** The ranges the backend takes for the memory limits */
const MAX_ENTRIES = {min: 10, max: 2000, step: 10}
const SLICE_CHARS = {min: 500, max: 12000, step: 250}

/** Who the user is to the model, and how the app reaches them */
export const ProfileTab = ({draft, onDraft}: SettingsDraftProps) => {
    const onToggleNotifications = async (enabled: boolean) => {
        onDraft({notifications: enabled ? await requestNotificationPermission() : false})
    }
    return (
        <>
            <SettingsSection title="Profile">
                <NameTimezoneRow name={draft.name} onNameChanged={(name) => onDraft({name})}
                                 timezoneText={draft.timezoneText} onTimezoneChanged={(timezoneText) => onDraft({timezoneText})}/>
                <NotificationsField enabled={draft.notifications} onToggle={onToggleNotifications}/>
            </SettingsSection>
            <SettingsSection title="Memory">
                <MemoryEnabledField enabled={draft.memoryEnabled} onToggle={(memoryEnabled) => onDraft({memoryEnabled})}/>
                <SliderField label="How much to keep" {...MAX_ENTRIES} value={draft.memoryMaxEntries}
                             onChanged={(memoryMaxEntries) => onDraft({memoryMaxEntries})}
                             shown={`${draft.memoryMaxEntries} entries`}
                             help="How many of the entries the model wrote are kept; past this the oldest of them are dropped. The entries you write yourself are never counted or dropped."/>
                <SliderField label="How much goes into a chat" {...SLICE_CHARS} value={draft.memorySliceChars}
                             onChanged={(memorySliceChars) => onDraft({memorySliceChars})}
                             shown={`${draft.memorySliceChars.toLocaleString()} characters`}
                             help="How much of the memory a chat's rules take, about four characters to the token. It sits in front of every request, so a bigger slice leaves less room for the chat. Only whole entries go in: how you like things done first, then what is known about you."/>
                <MemoryList/>
            </SettingsSection>
        </>
    )
};
