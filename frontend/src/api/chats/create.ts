import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ToolMode} from '../settings/types'
import type {ChatOut} from './types'

/** What a new chat starts on instead of the user's default: a launch profile, or a model of a provider, and a tool selection */
export interface ChatStart {
    launchProfileId?: number
    model?: string
    provider?: string
    /** Sent together: left out, the chat copies the user's tool selection */
    toolMode?: ToolMode
    tools?: string[]
}

/** Creates a new chat with the given name, on the default model unless `start` names another */
export const createChat = async (name: string, start?: ChatStart): Promise<ChatOut> => {
    const {data} = await axios.post<ChatOut>(`${BACKEND_URL}/api/chats`, {
        name,
        launch_profile_id: start?.launchProfileId,
        model: start?.model,
        provider: start?.provider,
        ...(start?.toolMode != null && start.tools != null ? {tool_mode: start.toolMode, tools: start.tools} : {}),
    })
    return data
};
