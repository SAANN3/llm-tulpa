import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ThinkingCapability} from '../agent/types'

/** What the active model supports for the think option */
export const getThinkingCapability = async (): Promise<ThinkingCapability> => {
    const {data} = await axios.get<ThinkingCapability>(`${BACKEND_URL}/api/llm/thinking_capability`)
    return data
};
