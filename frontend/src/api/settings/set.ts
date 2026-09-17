import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Settings} from './types'

/** Persists user settings */
export const setSettings = async (settings: Settings): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/settings`, settings)
};
