import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ToolChoice} from '../settings/types'

/** Replaces the tool selection this chat sends, from its next request on. Refused (409) while a run is going. */
export const setChatTools = async (chatId: number, choice: ToolChoice): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/tools`, {chat_id: chatId, mode: choice.mode, tools: choice.tools})
}
