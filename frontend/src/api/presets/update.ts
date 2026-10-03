import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Preset, PresetIn} from './types'

/** Replaces a preset's settings as a whole */
export const updatePreset = async (id: number, preset: PresetIn): Promise<Preset> => {
    const {data} = await axios.post<Preset>(`${BACKEND_URL}/api/presets/update`, {id, ...preset})
    return data
};
