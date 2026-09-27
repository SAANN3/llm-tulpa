export interface Settings {
    name: string | null
    /** UTC offset in whole hours (e.g. `-5`, `9`), not an IANA timezone name. */
    timezone: number | null
    notifications_enabled: boolean
    theme: string | null
    language: string
    llm_provider: string
    active_model: string | null
}

export interface SettingsUpdate {
    name?: string
    timezone?: number
    notifications_enabled?: boolean
    theme?: string
    language?: string
    llm_provider?: string
    active_model?: string
}

export interface SystemPromptOut {
    /** The user's custom system prompt — null while the built-in default applies. */
    custom: string | null
    /** The built-in default system prompt. */
    default: string
}

export interface SystemPromptUpdate {
    /** The new prompt — null resets it to the built-in default. */
    prompt: string | null
}
