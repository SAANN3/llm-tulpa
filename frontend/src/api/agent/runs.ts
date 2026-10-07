import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {RunningChat} from './types'

/** The caller's chats that have a run going on right now, each with its turn state */
export const getRuns = async (): Promise<RunningChat[]> => {
    const {data} = await axios.get<RunningChat[]>(`${BACKEND_URL}/api/agent/runs`)
    return data
};
