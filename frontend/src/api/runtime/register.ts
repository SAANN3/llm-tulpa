import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ManagedModel} from './types'

/** Registers a `.gguf` file as a model (owner only); a first launch profile is made for it */
export const registerModel = async (file: string, displayName?: string, projector?: string): Promise<ManagedModel> => {
    const {data} = await axios.post<{ model: ManagedModel }>(`${BACKEND_URL}/api/runtime/models`, {
        file,
        display_name: displayName || undefined,
        projector: projector || undefined,
    })
    return data.model
};
