import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Removes a message and everything after it; resolves to how many messages went */
export async function rewindChat(chatId: number, messageId: number): Promise<number> {
    const {data} = await axios.post<{ deleted: number }>(`${BACKEND_URL}/api/chats/rewind`, {
        chat_id: chatId,
        message_id: messageId,
    })
    return data.deleted
}
