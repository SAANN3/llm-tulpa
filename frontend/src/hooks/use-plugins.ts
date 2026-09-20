import {useEffect, useState} from 'react'
import {setPluginEnabled as setPluginEnabledApi} from '../api/plugins/enable'
import {getPluginHelp} from '../api/plugins/get-help.ts'
import {getPlugins} from '../api/plugins/list'
import {setPluginSettings as setPluginSettingsApi} from '../api/plugins/set-settings.ts'
import {getPluginSettingsSchema} from '../api/plugins/settings-schema.ts'
import type {PluginInfo, PropertyInfo} from '../api/plugins/types'

const isSamePlugin = (plugin: PluginInfo, pluginName: string, pluginSubname: string): boolean => plugin.plugin_name === pluginName && plugin.plugin_subname === pluginSubname;

/** Every registered plugin, fetched once on mount and patched in place on changes */
export const usePlugins = () => {
    const [plugins, setPlugins] = useState<PluginInfo[]>([])
    const [loading, setLoading] = useState(true)

    useEffect(() => {
        getPlugins()
            .then(setPlugins)
            .finally(() => setLoading(false))
    }, [])

    const setEnabled = async (pluginName: string, pluginSubname: string, enabled: boolean) => {
        await setPluginEnabledApi({pluginName, pluginSubname, enabled})
        setPlugins((prev) => prev.map((p) => (isSamePlugin(p, pluginName, pluginSubname) ? {...p, enabled} : p)))
    }

    const setSettings = async (pluginName: string, pluginSubname: string, settings: Record<string, unknown>) => {
        await setPluginSettingsApi({pluginName, pluginSubname, settings})
        setPlugins((prev) => prev.map((p) => (isSamePlugin(p, pluginName, pluginSubname) ? {...p, settings} : p)))
    }

    const getSchema = (pluginName: string, pluginSubname: string): Promise<PropertyInfo[]> =>
        getPluginSettingsSchema({pluginName, pluginSubname})

    const getHelp = (pluginName: string, pluginSubname: string): Promise<string> =>
        getPluginHelp({pluginName, pluginSubname})

    return {plugins, loading, setEnabled, setSettings, getSchema, getHelp}
};
