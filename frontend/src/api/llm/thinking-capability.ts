import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ThinkingCapability} from '../agent/types'

/** A model to ask about instead of a chat's: for a chat that doesn't exist yet */
export interface ThinkingModel {
    model: string
    provider: string
}

/** What a chat's bound model supports for the think option (the given model, else the user's default model, when no chat is given) */
export const getThinkingCapability = async (chatId?: number, model?: ThinkingModel | null): Promise<ThinkingCapability> => {
    const params = chatId != null ? {chat_id: chatId} : model ? {model: model.model, provider: model.provider} : undefined
    const {data} = await axios.get<ThinkingCapability>(`${BACKEND_URL}/api/llm/thinking_capability`, {params})
    return data
};
