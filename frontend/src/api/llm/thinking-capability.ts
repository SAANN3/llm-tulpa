import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ThinkingCapability} from '../agent/types'

/** What a chat's bound model supports for the think option (the user's default model when no chat is given) */
export const getThinkingCapability = async (chatId?: number): Promise<ThinkingCapability> => {
    const {data} = await axios.get<ThinkingCapability>(`${BACKEND_URL}/api/llm/thinking_capability`, {
        params: chatId == null ? undefined : {chat_id: chatId},
    })
    return data
};
