import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {StagedPlugin} from './types'

/** Sends a .tulpa file to be unpacked and described; nothing of it runs */
export const stageExternalPluginFile = async (file: File): Promise<StagedPlugin> => {
    const form = new FormData()
    form.append('file', file, file.name)
    const {data} = await axios.post<StagedPlugin>(`${BACKEND_URL}/api/plugins/external/stage/upload`, form)
    return data
};
