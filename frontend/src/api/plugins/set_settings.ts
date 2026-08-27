import axios from 'axios'

import { BACKEND_URL } from '../../config'

export interface SetPluginSettingsBody {
  pluginName: string
  pluginSubname: string
  settings: Record<string, unknown>
}

/**
 * Sets (or replaces) a plugin's settings, rebuilding its live instance from them —
 * works both for first-time configuration and for changing existing settings. Doesn't
 * change whether the plugin is enabled — see `setPluginEnabled`. Mirrors
 * `POST /api/plugins/settings`.
 */
export async function setPluginSettings(body: SetPluginSettingsBody): Promise<void> {
  await axios.post(`${BACKEND_URL}/api/plugins/settings`, {
    plugin_name: body.pluginName,
    plugin_subname: body.pluginSubname,
    settings: body.settings,
  })
}
