import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Stops the chat's run: the model call in flight is dropped and nothing of it is stored */
export const stopTurn = async (chatId: number): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/agent/stop`, {chat_id: chatId})
};
