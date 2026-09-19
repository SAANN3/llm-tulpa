import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface GreetOut {
    response: string
    model: string
    created_at: string
    thinking: string | null
}

/**
 * Generates a greeting for the empty chat landing page. `signal` lets a caller cancel the
 * request outright (not just ignore its result): a cache miss can take many seconds, and a
 * caller that has navigated away shouldn't leave it holding a browser connection slot.
 */
export const greet = async (signal?: AbortSignal): Promise<GreetOut> => {
    const {data} = await axios.post<GreetOut>(`${BACKEND_URL}/api/prompts/greet`, undefined, {signal})
    return data
};
