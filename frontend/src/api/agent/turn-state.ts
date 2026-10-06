import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {TurnState} from './types'

/** What the chat's turn is doing: idle, running (since when, tokens so far) or waiting for permission */
export const getTurnState = async (chatId: number): Promise<TurnState> => {
    const {data} = await axios.get<TurnState>(`${BACKEND_URL}/api/agent/turn`, {params: {chat_id: chatId}})
    return data
};
