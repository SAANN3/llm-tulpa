import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {Decision, ThinkChoice} from './types'

/** Answers the permission prompt the chat is waiting at; the run continues in the background */
export const answer = async (chatId: number, decisions: Decision[], think: ThinkChoice = true): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/agent/answer`, {chat_id: chatId, decisions, think})
};
