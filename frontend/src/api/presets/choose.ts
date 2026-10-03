import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Chooses which preset samples the caller's calls to a model; null goes back to the server's defaults */
export const choosePreset = async (modelId: number, presetId: number | null): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/presets/choose`, {model_id: modelId, preset_id: presetId})
};
