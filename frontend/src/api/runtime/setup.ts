import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Records that the owner has been through the current setup (owner only) */
export const completeSetup = async (): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/runtime/setup-complete`)
};

/** Moves every chat on an Ollama model onto a launch profile of the managed llama.cpp (owner only) */
export const rebindChats = async (profileId: number): Promise<number> => {
    const {data} = await axios.post<{ moved: number }>(`${BACKEND_URL}/api/runtime/rebind-chats`, {profile_id: profileId})
    return data.moved
};
