import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface GreetOut {
    response: string
    model: string
    created_at: string
    thinking: string | null
}

/** Generates a greeting for the empty chat landing page */
export const greet = async (signal?: AbortSignal): Promise<GreetOut> => {
    const {data} = await axios.post<GreetOut>(`${BACKEND_URL}/api/prompts/greet`, undefined, {signal})
    return data
};
