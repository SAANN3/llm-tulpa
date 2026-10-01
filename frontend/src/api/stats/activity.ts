import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Activity, StatsRange} from './types'

/** When the user is active, chats started, and background jobs over the last `days` days or `months` months */
export const getActivity = async (range: StatsRange): Promise<Activity> => {
    const {data} = await axios.get<Activity>(`${BACKEND_URL}/api/stats/activity`, {params: range})
    return data
};
