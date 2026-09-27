import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SystemPromptOut} from './types'

/** Fetches the user's custom system prompt alongside the built-in default */
export const getSystemPrompt = async (): Promise<SystemPromptOut> => {
    const {data} = await axios.get<SystemPromptOut>(`${BACKEND_URL}/api/settings/system-prompt`)
    return data
};
