import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {UseToolOut} from './types'

/** Runs the next pending tool call for a chat and persists its result */
export const useTool = async (chatId: number, scope?: unknown): Promise<UseToolOut> => {
    const {data} = await axios.post<UseToolOut>(`${BACKEND_URL}/api/agent/use_tool`, {
        chat_id: chatId,
        scope: scope ?? null,
    })
    return data
};
