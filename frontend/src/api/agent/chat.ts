import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ChatOut, ThinkChoice} from './types'

/** Sends the next chat message and returns the model's reply */
export const chat = async (
    chatId: number,
    prompt: string,
    think: ThinkChoice = true,
    images: string[] = [],
    fileIds: number[] = [],
): Promise<ChatOut> => {
    const {data} = await axios.post<ChatOut>(`${BACKEND_URL}/api/agent/chat`, {
        chat_id: chatId,
        prompt,
        think,
        images,
        file_ids: fileIds,
    })
    return data
};
