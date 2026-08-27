import { useEffect, useState } from 'react'

import { setPluginEnabled as setPluginEnabledApi } from '../api/plugins/enable'
import { getPluginHelp } from '../api/plugins/get_help'
import { getPlugins } from '../api/plugins/list'
import { setPluginSettings as setPluginSettingsApi } from '../api/plugins/set_settings'
import { getPluginSettingsSchema } from '../api/plugins/settings_schema'
import type { PluginInfo, PropertyInfo } from '../api/plugins/types'

function isSamePlugin(plugin: PluginInfo, pluginName: string, pluginSubname: string): boolean {
  return plugin.plugin_name === pluginName && plugin.plugin_subname === pluginSubname
}

/**
 * Every registered plugin, enabled and disabled alike, fetched once on mount.
 * `setEnabled`/`setSettings` call the backend then patch the matching entry in place
 * rather than refetching the whole list, same pattern as `useChats`.
 */
export function usePlugins() {
  const [plugins, setPlugins] = useState<PluginInfo[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    getPlugins()
      .then(setPlugins)
      .finally(() => setLoading(false))
  }, [])

  const setEnabled = async (pluginName: string, pluginSubname: string, enabled: boolean) => {
    await setPluginEnabledApi({ pluginName, pluginSubname, enabled })
    setPlugins((prev) => prev.map((p) => (isSamePlugin(p, pluginName, pluginSubname) ? { ...p, enabled } : p)))
  }

  const setSettings = async (pluginName: string, pluginSubname: string, settings: Record<string, unknown>) => {
    await setPluginSettingsApi({ pluginName, pluginSubname, settings })
    setPlugins((prev) => prev.map((p) => (isSamePlugin(p, pluginName, pluginSubname) ? { ...p, settings } : p)))
  }

  /** A plugin's settings schema, for rendering its settings form — fetched fresh on
   * every call rather than cached, since it's only needed on demand (e.g. opening a
   * plugin's settings) and the schema itself never changes between calls. */
  const getSchema = (pluginName: string, pluginSubname: string): Promise<PropertyInfo[]> =>
    getPluginSettingsSchema({ pluginName, pluginSubname })

  /** A plugin's own how-to-use message — same on-demand, uncached fetch pattern as
   * `getSchema`. */
  const getHelp = (pluginName: string, pluginSubname: string): Promise<string> =>
    getPluginHelp({ pluginName, pluginSubname })

  return { plugins, loading, setEnabled, setSettings, getSchema, getHelp }
}
