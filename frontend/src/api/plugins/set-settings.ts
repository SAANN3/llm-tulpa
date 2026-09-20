import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface SetPluginSettingsBody {
    pluginName: string
    pluginSubname: string
    settings: Record<string, unknown>
}

/** Sets or replaces a plugin's settings */
export const setPluginSettings = async (body: SetPluginSettingsBody): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/plugins/settings`, {
        plugin_name: body.pluginName,
        plugin_subname: body.pluginSubname,
        settings: body.settings,
    })
};
