import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {PluginTabInfo} from './types.ts'

/** The Settings tabs the enabled plugins declare */
export const getPluginTabs = async (): Promise<PluginTabInfo[]> => {
    const {data} = await axios.get<PluginTabInfo[]>(`${BACKEND_URL}/api/plugins/tabs`)
    return data
};
