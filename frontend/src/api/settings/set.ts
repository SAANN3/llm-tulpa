import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SettingsUpdate} from './types'

/** Applies a partial update to the user's settings */
export const setSettings = async (update: SettingsUpdate): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/settings`, update)
};
