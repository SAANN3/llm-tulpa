import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Deletes an external plugin's folder and record; `removeData` deletes what it kept too (settings, database tables, files) */
export const uninstallExternalPlugin = async (pluginName: string, pluginSubname: string, removeData: boolean): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/plugins/external/uninstall`, {plugin_name: pluginName, plugin_subname: pluginSubname, remove_data: removeData})
};
