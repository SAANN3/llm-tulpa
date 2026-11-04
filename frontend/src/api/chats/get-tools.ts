import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ToolChoice} from '../settings/types'

/** The tool selection this chat sends */
export const getChatTools = async (chatId: number): Promise<ToolChoice> => {
    const {data} = await axios.get<ToolChoice>(`${BACKEND_URL}/api/chats/tools`, {params: {chat_id: chatId}})
    return data
}
