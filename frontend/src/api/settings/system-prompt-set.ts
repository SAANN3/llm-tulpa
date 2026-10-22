import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {SystemPromptUpdate} from './types'

/** Sets the user's custom system prompt, which replaces the built-in default or is added after it — null resets it to the default */
export const setSystemPrompt = async (prompt: string | null, append: boolean): Promise<void> => {
    const body: SystemPromptUpdate = {prompt, append}
    await axios.post(`${BACKEND_URL}/api/settings/system-prompt`, body)
};
