import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {PluginInfo} from './types'

/** Fetches every registered plugin, enabled and disabled alike */
export const getPlugins = async (): Promise<PluginInfo[]> => {
    const {data} = await axios.get<PluginInfo[]>(`${BACKEND_URL}/api/plugins`)
    return data
};
