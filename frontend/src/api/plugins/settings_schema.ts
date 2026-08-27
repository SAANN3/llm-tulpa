import axios from 'axios'

import { BACKEND_URL } from '../../config'
import type { PropertyInfo } from './types'

export interface GetPluginSettingsSchemaQuery {
  pluginName: string
  pluginSubname: string
}

/**
 * A plugin's settings schema, in the same shape used for tool-calling args — what a
 * settings form would render from. Present for every registered plugin, even one
 * that hasn't been configured yet. Mirrors `GET /api/plugins/settings_schema`.
 */
export async function getPluginSettingsSchema(query: GetPluginSettingsSchemaQuery): Promise<PropertyInfo[]> {
  const { data } = await axios.get<PropertyInfo[]>(`${BACKEND_URL}/api/plugins/settings_schema`, {
    params: { plugin_name: query.pluginName, plugin_subname: query.pluginSubname },
  })

  return data
}
