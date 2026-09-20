import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ChatOut, ThinkChoice} from './types'

/** Reports finished background jobs to the chat's model, or resolves null when there was nothing to report */
export async function jobNotices(chatId: number, think: ThinkChoice = true): Promise<ChatOut | null> {
    const {data} = await axios.post<ChatOut | null>(`${BACKEND_URL}/api/agent/job_notices`, {
        chat_id: chatId,
        think,
    })
    return data
}
