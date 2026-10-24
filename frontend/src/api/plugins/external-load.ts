import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ExternalPlugin} from './types'

/** Loads the external plugin in its folder while the backend runs */
export const loadExternalPlugin = async (pluginName: string, pluginSubname: string): Promise<ExternalPlugin> => {
    const {data} = await axios.post<ExternalPlugin>(`${BACKEND_URL}/api/plugins/external/load`, {plugin_name: pluginName, plugin_subname: pluginSubname})
    return data
};
