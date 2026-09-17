import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface SetPluginEnabledBody {
    pluginName: string
    pluginSubname: string
    enabled: boolean
}

/** Enables or disables a registered plugin */
export const setPluginEnabled = async (body: SetPluginEnabledBody): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/plugins/enable`, {
        plugin_name: body.pluginName,
        plugin_subname: body.pluginSubname,
        enabled: body.enabled,
    })
};
