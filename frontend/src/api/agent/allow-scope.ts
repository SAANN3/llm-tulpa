import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Grants a tool a permanent permission scope within a chat */
export const allowScope = async (chatId: number, toolName: string, scope: unknown): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/agent/allow_scope`, {
        chat_id: chatId,
        tool_name: toolName,
        scope,
    })
};
