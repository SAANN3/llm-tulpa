import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** The text of one file of a staged plugin */
export const getStagedPluginFile = async (id: string, path: string): Promise<string> => {
    const {data} = await axios.get<string>(`${BACKEND_URL}/api/plugins/external/stage/file`, {params: {id, path}, responseType: 'text', transformResponse: (raw) => raw})
    return data
};
