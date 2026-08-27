import axios from 'axios'

import { BACKEND_URL } from '../../config'

export interface SetPluginEnabledBody {
  pluginName: string
  pluginSubname: string
  enabled: boolean
}

/**
 * Enables or disables a registered plugin. `enabled: true` fails if the plugin has no
 * settings configured yet — set those first via `setPluginSettings`. Mirrors
 * `POST /api/plugins/enable`.
 */
export async function setPluginEnabled(body: SetPluginEnabledBody): Promise<void> {
  await axios.post(`${BACKEND_URL}/api/plugins/enable`, {
    plugin_name: body.pluginName,
    plugin_subname: body.pluginSubname,
    enabled: body.enabled,
  })
}
