export interface Settings {
    name: string | null
    /** UTC offset in whole hours (e.g. `-5`, `9`), not an IANA timezone name. */
    timezone: number | null
    notifications_enabled: boolean
    theme: string | null
    language: string
    /** Tool-permission prompts are approved automatically instead of waiting for the user. */
    auto_confirm: boolean
    /** Old thinking traces are shortened to their tail in a long chat, and the newest are replayed longer. */
    trim_old_thinking: boolean
    /** Whether a Hugging Face token is set (the token itself is never sent back) */
    has_hf_token: boolean
    llm_provider: string
    active_model: string | null
    /** The launch profile of that model new chats start on; null means its first one */
    launch_profile_id: number | null
}

export interface SettingsUpdate {
    name?: string
    timezone?: number
    notifications_enabled?: boolean
    theme?: string
    language?: string
    auto_confirm?: boolean
    trim_old_thinking?: boolean
    /** A Hugging Face access token; empty clears it */
    hf_token?: string
    llm_provider?: string
    active_model?: string
    /** The launch profile of `active_model`; leaving it out with a new model clears the choice */
    launch_profile_id?: number
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
