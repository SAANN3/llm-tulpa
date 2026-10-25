export type PropertyType = 'string' | 'number' | 'integer' | 'boolean' | 'array' | 'object'

export interface PropertyInfo {
    name: string
    property_type: PropertyType
    description: string
    required: boolean
}

/** The model chosen for a plugin's chats */
export interface PluginModel {
    provider: string
    model: string
    /** The launch profile chosen with it (llama.cpp models only) */
    launch_profile_id: number | null
}

export interface PluginInfo {
    plugin_name: string
    plugin_subname: string
    enabled: boolean
    settings: Record<string, unknown> | null
    /** What the plugin does, in one short line */
    summary: string
    /** Whether the plugin has chats of its own, so a model can be chosen for them */
    uses_agent: boolean
    /** The model its chats run on; null leaves them on the model they have */
    model: PluginModel | null
}

/** A Settings tab an enabled plugin declares */
export interface PluginTabInfo {
    plugin_name: string
    plugin_subname: string
    /** Its place in the address: `/settings/<id>` */
    id: string
    title: string
}

export interface TabRowSwitch {
    action: string
    on: boolean
}

export interface TabRowButton {
    action: string
    label: string
    /** Asked in a popup before the button's action is sent */
    confirm: string | null
    /** Opens a file picker; the chosen files are uploaded with the row's id instead of the action being sent */
    upload: boolean
}

export interface TabRow {
    id: string
    title: string
    subtitle: string | null
    /** A short label beside the title, such as where the item came from */
    badge: string | null
    switch: TabRowSwitch | null
    buttons: TabRowButton[]
}

/** One thing in a section; every control carries the `action` it sends back to the plugin */
export type TabBlock =
    | { type: 'text'; text: string; dim: boolean }
    | { type: 'switch'; action: string; label: string; help: string | null; on: boolean }
    | { type: 'items'; empty: string; rows: TabRow[] }
    | { type: 'drop'; action: string; label: string; help: string | null; accept: string[] }

export interface TabSection {
    title: string | null
    blocks: TabBlock[]
}

/** What a plugin's tab shows the user now */
export interface TabLayout {
    sections: TabSection[]
    /** A line after an action: what was done, or why it was not */
    message: string | null
    /** A text to show in a popup, such as a skill's instructions */
    popup: { title: string; text: string } | null
}

/** How an external plugin's process is doing */
export type ExternalProcessState = 'stopped' | 'running' | 'restarting' | 'failed'

export interface ExternalProcess {
    state: ExternalProcessState
    pid: number | null
    /** How many times a new process was started after a crash */
    restarts: number
    last_error: string | null
    /** The end of what the plugin wrote to its log, oldest first */
    logs: string[]
}

/** How an external plugin is doing as a whole: `unavailable` could not be started (the reason is `last_error`), `failing`
 * crashed too often and was switched off, `unloaded` was stopped by the owner */
export type ExternalState = 'ok' | 'unavailable' | 'failing' | 'unloaded'

/** An external plugin: a separate program the backend runs, installed from a file or a git address or put in its folder by hand */
export interface ExternalPlugin {
    name: string
    subname: string
    version: string | null
    kind: string | null
    /** `folder` (put there by hand), `file` or `git` */
    source_type: string
    source_url: string | null
    source_ref: string | null
    source_sha256: string | null
    state: ExternalState
    last_error: string | null
    installed_at: string
    updated_at: string
    /** `null` for a plugin that was never loaded */
    process: ExternalProcess | null
}

export interface StagedSource {
    source_type: string
    url: string | null
    git_ref: string | null
    commit: string | null
    sha256: string | null
}

export interface StagedFile {
    path: string
    size: number
}

/** A plugin that has been unpacked and looked at: nothing of it has run, and nothing is installed until the owner says so */
export interface StagedPlugin {
    id: string
    source: StagedSource
    name: string
    subname: string
    version: string
    kind: string
    summary: string
    help_message: string
    /** The text of its plugin.toml */
    plugin_toml: string
    readme: string | null
    build: string | null
    run: string
    check: string | null
    /** The names of the settings it asks for */
    settings: string[]
    has_tab: boolean
    has_database: boolean
    files: StagedFile[]
    files_truncated: boolean
    total_bytes: number
    action: 'install' | 'update'
    installed_version: string | null
    /** Why it cannot be installed; empty when it can */
    problems: string[]
}

export interface InstallTask {
    id: string
    name: string
    subname: string
    state: 'running' | 'done' | 'failed'
    /** What it is doing now */
    step: string
    log: string[]
    error: string | null
}
