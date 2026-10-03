import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Rebinds a chat to a launch profile, effective from its next turn */
export const setChatProfile = async (chatId: number, profileId: number): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/profile`, {chat_id: chatId, profile_id: profileId})
};
