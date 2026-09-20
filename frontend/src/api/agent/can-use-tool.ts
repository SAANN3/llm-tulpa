import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {CanUseTool} from './types'

/** Pending tool calls for a chat, without running them */
export const canUseTool = async (chatId: number): Promise<CanUseTool> => {
    const {data} = await axios.post<CanUseTool>(`${BACKEND_URL}/api/agent/can_use_tool`, {
        chat_id: chatId,
    })
    return data
};
