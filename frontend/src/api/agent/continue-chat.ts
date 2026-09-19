import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ChatOut, ThinkChoice} from './types'

/** Continues a chat with no new message, for getting a reply after a tool result */
export async function continueChat(chatId: number, think: ThinkChoice = true): Promise<ChatOut> {
    const {data} = await axios.post<ChatOut>(`${BACKEND_URL}/api/agent/continue`, {
        chat_id: chatId,
        think,
    })
    return data
}
