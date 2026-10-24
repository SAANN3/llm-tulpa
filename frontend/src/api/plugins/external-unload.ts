import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Stops an external plugin and takes it out of the list until it is loaded again; its files and data stay */
export const unloadExternalPlugin = async (pluginName: string, pluginSubname: string): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/plugins/external/unload`, {plugin_name: pluginName, plugin_subname: pluginSubname})
};
