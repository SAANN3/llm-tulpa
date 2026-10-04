import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ChatOut} from './types'

/** What a new chat starts on instead of the user's default: a launch profile, or a model of a provider */
export interface ChatStart {
    launchProfileId?: number
    model?: string
    provider?: string
}

/** Creates a new chat with the given name, on the default model unless `start` names another */
export const createChat = async (name: string, start?: ChatStart): Promise<ChatOut> => {
    const {data} = await axios.post<ChatOut>(`${BACKEND_URL}/api/chats`, {
        name,
        launch_profile_id: start?.launchProfileId,
        model: start?.model,
        provider: start?.provider,
    })
    return data
};
