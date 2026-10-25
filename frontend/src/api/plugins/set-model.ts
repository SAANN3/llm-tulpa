import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface SetPluginModelBody {
    pluginName: string
    pluginSubname: string
    /** A launch profile (llama.cpp), which names its model; leave out with `model` to take the choice away */
    profileId?: number
    /** A model by name, for a provider with no launch profiles (Ollama) */
    model?: string
    provider?: string
}

/** Chooses the model a plugin's chats run on, or removes the choice */
export const setPluginModel = async (body: SetPluginModelBody): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/plugins/model`, {
        plugin_name: body.pluginName,
        plugin_subname: body.pluginSubname,
        profile_id: body.profileId,
        model: body.model,
        provider: body.provider,
    })
};
