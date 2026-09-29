import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {NoticeOut} from './types'

/** Persists the notices for jobs that finished since the model was last told (without calling the model) and returns them; empty when there was nothing to report */
export async function jobNotices(chatId: number): Promise<NoticeOut[]> {
    const {data} = await axios.post<{notices: NoticeOut[]}>(`${BACKEND_URL}/api/agent/job_notices`, {
        chat_id: chatId,
    })
    return data.notices
}
