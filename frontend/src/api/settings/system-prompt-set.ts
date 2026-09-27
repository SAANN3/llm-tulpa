import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SystemPromptUpdate} from './types'

/** Sets the user's custom system prompt — or resets it to the built-in default when null */
export const setSystemPrompt = async (prompt: string | null): Promise<void> => {
    const body: SystemPromptUpdate = {prompt}
    await axios.post(`${BACKEND_URL}/api/settings/system-prompt`, body)
};
