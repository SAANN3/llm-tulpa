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
}
