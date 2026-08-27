import axios from 'axios'

import { BACKEND_URL } from '../../config'

export interface GetPluginHelpQuery {
  pluginName: string
  pluginSubname: string
}

/**
 * A plugin's own info/help message — how to use it, step by step. Present for every
 * registered plugin, even one that hasn't been configured yet. Mirrors
 * `GET /api/plugins/help`.
 */
export async function getPluginHelp(query: GetPluginHelpQuery): Promise<string> {
  const { data } = await axios.get<string>(`${BACKEND_URL}/api/plugins/help`, {
    params: { plugin_name: query.pluginName, plugin_subname: query.pluginSubname },
  })

  return data
}
