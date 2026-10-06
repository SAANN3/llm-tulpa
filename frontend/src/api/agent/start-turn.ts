import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {StartTurnOut, ThinkChoice} from './types'

/** Stores the prompt as the user's next message and starts a run that answers it; returns at once. A 409 means the chat already has a run. */
export const startTurn = async (
    chatId: number,
    prompt: string,
    think: ThinkChoice = true,
    images: string[] = [],
    fileIds: number[] = [],
): Promise<StartTurnOut> => {
    const {data} = await axios.post<StartTurnOut>(`${BACKEND_URL}/api/agent/turn`, {
        chat_id: chatId,
        prompt,
        think,
        images,
        file_ids: fileIds,
    })
    return data
};
