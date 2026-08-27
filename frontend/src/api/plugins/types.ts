export type PropertyType = 'string' | 'number' | 'integer' | 'boolean' | 'array' | 'object'

/** One settings field's schema — same shape used for tool-calling args on the backend. */
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
  /** `null` for a plugin that's registered but hasn't been given settings yet. */
  settings: Record<string, unknown> | null
}
