import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ImportResult, PresetsFile} from './types'

/** Adds the presets of an exported file to the caller's own; nothing already there is overwritten */
export const importPresets = async (file: PresetsFile): Promise<ImportResult> => {
    const {data} = await axios.post<ImportResult>(`${BACKEND_URL}/api/presets/import`, file)
    return data
};
