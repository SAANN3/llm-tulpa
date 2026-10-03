import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {PresetsFile} from './types'

/** All of the caller's presets as a file to keep or share */
export const exportPresets = async (): Promise<PresetsFile> => {
    const {data} = await axios.get<PresetsFile>(`${BACKEND_URL}/api/presets/export`)
    return data
};
