import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {LaunchProfile, LaunchProfileIn} from './types'

/** Changes a launch profile (owner only); a running server picks it up the next time the profile loads */
export const updateProfile = async (id: number, settings: LaunchProfileIn): Promise<LaunchProfile> => {
    const {data} = await axios.post<LaunchProfile>(`${BACKEND_URL}/api/profiles/update`, {id, ...settings})
    return data
};
