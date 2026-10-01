import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ChatOut, ThinkChoice} from './types'

/** Has the model answer again and replaces the chat's last reply (`messageId`) with the new one */
export async function regenerateChat(chatId: number, messageId: number, think: ThinkChoice = true): Promise<ChatOut> {
    const {data} = await axios.post<ChatOut>(`${BACKEND_URL}/api/agent/regenerate`, {
        chat_id: chatId,
        message_id: messageId,
        think,
    })
    return data
}
