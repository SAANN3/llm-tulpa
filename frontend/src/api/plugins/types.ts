export type PropertyType = 'string' | 'number' | 'integer' | 'boolean' | 'array' | 'object'

export interface PropertyInfo {
    name: string
    property_type: PropertyType
    description: string
    required: boolean
}

export interface PluginInfo {
    plugin_name: string
    plugin_subname: string
    enabled: boolean
    settings: Record<string, unknown> | null
    /** What the plugin does, in one short line */
    summary: string
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
