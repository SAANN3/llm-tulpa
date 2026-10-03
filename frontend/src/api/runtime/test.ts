import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SpeedTest} from './types'

/** Loads the profile if needed, sends one prompt and reports how fast it ran */
export const runSpeedTest = async (profileId: number, prompt?: string): Promise<SpeedTest> => {
    const {data} = await axios.post<SpeedTest>(`${BACKEND_URL}/api/runtime/test`, {profile_id: profileId, prompt}, {timeout: 0})
    return data
};
