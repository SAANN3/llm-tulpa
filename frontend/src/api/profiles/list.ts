import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {LaunchProfile} from './types'

/** Every launch profile; shared by all users */
export const listProfiles = async (): Promise<LaunchProfile[]> => {
    const {data} = await axios.get<{ profiles: LaunchProfile[] }>(`${BACKEND_URL}/api/profiles`)
    return data.profiles
};
