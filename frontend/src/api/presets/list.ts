import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Preset} from './types'

/** The caller's presets, with the one chosen for `modelId` when given */
export const listPresets = async (modelId?: number): Promise<{ presets: Preset[]; chosen: number | null }> => {
    const {data} = await axios.get<{ presets: Preset[]; chosen: number | null }>(`${BACKEND_URL}/api/presets`, {
        params: modelId == null ? {} : {model_id: modelId},
    })
    return data
};
