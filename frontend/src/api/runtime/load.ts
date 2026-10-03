import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RuntimeStatus} from './types'

/** Loads a launch profile and returns once the server is ready; 423 when another user has a turn on the loaded one */
export const loadProfile = async (profileId: number): Promise<RuntimeStatus> => {
    const {data} = await axios.post<RuntimeStatus>(`${BACKEND_URL}/api/runtime/load`, {profile_id: profileId}, {timeout: 0})
    return data
};
