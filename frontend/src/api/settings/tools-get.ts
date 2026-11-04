import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ToolChoice} from './types'

/** The tool selection a new chat copies */
export const getToolChoice = async (): Promise<ToolChoice> => {
    const {data} = await axios.get<ToolChoice>(`${BACKEND_URL}/api/settings/tools`)
    return data
}
