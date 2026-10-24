import axios from 'axios'
import {BACKEND_URL} from '../../config'
import {saveBlob} from '../../utils/save-blob.ts'

/** Saves an external plugin as a .tulpa file; it goes through axios because the route needs the Bearer token */
export const exportExternalPlugin = async (pluginName: string, pluginSubname: string, version: string | null): Promise<void> => {
    const {data} = await axios.get<Blob>(`${BACKEND_URL}/api/plugins/external/export`, {
        params: {plugin_name: pluginName, plugin_subname: pluginSubname},
        responseType: 'blob',
    })
    saveBlob(data, `${pluginName}-${pluginSubname}-${version ?? 'plugin'}.tulpa`)
};
