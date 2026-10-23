import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {TabLayout} from './types.ts'

/** What the user did in a plugin's tab: a control's action and what it sent; the layout as it is after */
export const sendPluginTabAction = async (pluginName: string, pluginSubname: string, action: string, args: Record<string, unknown>): Promise<TabLayout> => {
    const {data} = await axios.post<TabLayout>(`${BACKEND_URL}/api/plugins/tab/action`, {plugin_name: pluginName, plugin_subname: pluginSubname, action, args})
    return data
};
