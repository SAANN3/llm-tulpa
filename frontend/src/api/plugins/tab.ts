import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {TabLayout} from './types.ts'

/** What a plugin's tab shows the user now */
export const getPluginTab = async (pluginName: string, pluginSubname: string): Promise<TabLayout> => {
    const {data} = await axios.get<TabLayout>(`${BACKEND_URL}/api/plugins/tab`, {params: {plugin_name: pluginName, plugin_subname: pluginSubname}})
    return data
};
