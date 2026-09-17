import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {PropertyInfo} from './types'

export interface GetPluginSettingsSchemaQuery {
    pluginName: string
    pluginSubname: string
}

/** Fetches a plugin's settings schema */
export const getPluginSettingsSchema = async (query: GetPluginSettingsSchemaQuery): Promise<PropertyInfo[]> => {
    const {data} = await axios.get<PropertyInfo[]>(`${BACKEND_URL}/api/plugins/settings_schema`, {
        params: {plugin_name: query.pluginName, plugin_subname: query.pluginSubname},
    })
    return data
};
