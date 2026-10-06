import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ThinkChoice} from './types'

/** Has the model answer again where it answered last; the run goes on in the background and the new reply replaces the old one (`messageId`) when it is stored */
export async function regenerateChat(chatId: number, messageId: number, think: ThinkChoice = true): Promise<void> {
    await axios.post(`${BACKEND_URL}/api/agent/regenerate`, {
        chat_id: chatId,
        message_id: messageId,
        think,
    })
}
