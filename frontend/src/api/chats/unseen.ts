import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** The caller's chats with news: their last run ended in something not looked at yet */
export const getUnseenChats = async (): Promise<number[]> => {
    const {data} = await axios.get<{ chat_ids: number[] }>(`${BACKEND_URL}/api/chats/unseen`)
    return data.chat_ids
};
