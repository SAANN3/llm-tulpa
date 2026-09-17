import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Settings} from './types'

/** Fetches the persisted user settings */
export const getSettings = async (): Promise<Settings> => {
    const {data} = await axios.get<Settings>(`${BACKEND_URL}/api/settings`)
    return data
};
