import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ToolChoice} from './types'

/** Replaces the tool selection a new chat copies */
export const setToolChoice = async (choice: ToolChoice): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/settings/tools`, choice)
}
