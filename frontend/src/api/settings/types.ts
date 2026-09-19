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
