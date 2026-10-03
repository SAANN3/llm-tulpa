import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {LaunchProfile, LaunchProfileIn} from './types'

/** Adds a launch profile to a model (owner only) */
export const createProfile = async (modelId: number, settings: LaunchProfileIn): Promise<LaunchProfile> => {
    const {data} = await axios.post<LaunchProfile>(`${BACKEND_URL}/api/profiles`, {model_id: modelId, ...settings})
    return data
};
