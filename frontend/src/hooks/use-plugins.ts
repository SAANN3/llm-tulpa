import {useCallback, useEffect, useState} from 'react'
import {setPluginEnabled as setPluginEnabledApi} from '../api/plugins/enable'
import {getPluginHelp} from '../api/plugins/get-help.ts'
import {getPlugins} from '../api/plugins/list'
import {setPluginModel as setPluginModelApi} from '../api/plugins/set-model.ts'
import {setPluginSettings as setPluginSettingsApi} from '../api/plugins/set-settings.ts'
import {getPluginSettingsSchema} from '../api/plugins/settings-schema.ts'
import type {PluginInfo, PropertyInfo} from '../api/plugins/types'
import type {ModelChoice} from '../utils/model-choice.ts'

const isSamePlugin = (plugin: PluginInfo, pluginName: string, pluginSubname: string): boolean => plugin.plugin_name === pluginName && plugin.plugin_subname === pluginSubname;

/** Every registered plugin, fetched once on mount and patched in place on changes */
export const usePlugins = () => {
    const [plugins, setPlugins] = useState<PluginInfo[]>([])
    const [loading, setLoading] = useState(true)

    // Read again after something outside this hook changed the list (a plugin was installed, loaded or removed)
    const reload = useCallback(() => getPlugins().then(setPlugins), [])

    useEffect(() => {
        reload().finally(() => setLoading(false))
    }, [reload])

    const setEnabled = async (pluginName: string, pluginSubname: string, enabled: boolean) => {
        await setPluginEnabledApi({pluginName, pluginSubname, enabled})
        setPlugins((prev) => prev.map((p) => (isSamePlugin(p, pluginName, pluginSubname) ? {...p, enabled} : p)))
    }

    const setSettings = async (pluginName: string, pluginSubname: string, settings: Record<string, unknown>) => {
        await setPluginSettingsApi({pluginName, pluginSubname, settings})
        setPlugins((prev) => prev.map((p) => (isSamePlugin(p, pluginName, pluginSubname) ? {...p, settings} : p)))
    }

    /** Chooses the model for a plugin's chats (`null` takes the choice away); read again after, since the backend resolves the name */
    const setModel = async (pluginName: string, pluginSubname: string, choice: ModelChoice | null) => {
        await setPluginModelApi({
            pluginName,
            pluginSubname,
            ...(choice == null ? {} : choice.profileId != null ? {profileId: choice.profileId} : {model: choice.model, provider: choice.provider}),
        })
        await reload()
    }

    const getSchema = (pluginName: string, pluginSubname: string): Promise<PropertyInfo[]> =>
        getPluginSettingsSchema({pluginName, pluginSubname})

    const getHelp = (pluginName: string, pluginSubname: string): Promise<string> =>
        getPluginHelp({pluginName, pluginSubname})

    return {plugins, loading, reload, setEnabled, setSettings, setModel, getSchema, getHelp}
};
