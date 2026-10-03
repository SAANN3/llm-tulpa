import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {PresetIn} from './types'

/** The built-in starting points; copy one to make a preset of your own */
export const getPresetTemplates = async (): Promise<PresetIn[]> => {
    const {data} = await axios.get<{ templates: PresetIn[] }>(`${BACKEND_URL}/api/presets/templates`)
    return data.templates
};
