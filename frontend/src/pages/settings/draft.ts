import type {Settings} from '../../api/settings/types.ts'

/** The settings Save sends, as edited across the Behaviour and Profile tabs */
export interface SettingsDraft {
    name: string
    timezoneText: string
    notifications: boolean
    autoConfirm: boolean
    trimOldThinking: boolean
    useTools: boolean
    maxTurnStepsText: string
}

export interface SettingsDraftProps {
    draft: SettingsDraft
    onDraft: (change: Partial<SettingsDraft>) => void
}

const browserTimezoneOffsetHours = (): number => -new Date().getTimezoneOffset() / 60

export const draftOf = (settings: Settings | null): SettingsDraft => ({
    name: settings?.name ?? '',
    timezoneText: String(settings?.timezone ?? browserTimezoneOffsetHours()),
    notifications: settings?.notifications_enabled ?? false,
    autoConfirm: settings?.auto_confirm ?? false,
    trimOldThinking: settings?.trim_old_thinking ?? false,
    useTools: settings?.use_tools ?? true,
    maxTurnStepsText: settings?.max_turn_steps != null ? String(settings.max_turn_steps) : '',
})
