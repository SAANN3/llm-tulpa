import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {TabLayout} from './types.ts'

/** A file with the path it has in its folder (just its name when it was not in one) */
export interface PathedFile {
    path: string
    file: File
}

/** Files dropped on a drop zone of a plugin's tab; the layout as it is after */
export const uploadPluginTabFiles = async (pluginName: string, pluginSubname: string, action: string, files: PathedFile[]): Promise<TabLayout> => {
    const form = new FormData()
    form.append('plugin_name', pluginName)
    form.append('plugin_subname', pluginSubname)
    form.append('action', action)
    // The file's path goes in the part's filename, which is how a folder's files keep their folder
    for (const {path, file} of files) form.append('file', file, path)
    const {data} = await axios.post<TabLayout>(`${BACKEND_URL}/api/plugins/tab/upload`, form)
    return data
};
