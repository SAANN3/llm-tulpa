import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Preset, PresetIn} from './types'

/** Creates a preset for the caller; the name is unique per user and model */
export const createPreset = async (preset: PresetIn): Promise<Preset> => {
    const {data} = await axios.post<Preset>(`${BACKEND_URL}/api/presets`, preset)
    return data
};
