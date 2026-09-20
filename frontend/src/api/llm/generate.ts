import axios from 'axios'

import {BACKEND_URL} from '../../config'

export interface GenerateResponse {
    response: string
    model: string
    created_at: string
    thinking: string | null
}

/** Raw one-shot completion, no history and no tools */
export const generate = async (prompt: string, think = true): Promise<GenerateResponse> => {
    const {data} = await axios.post<GenerateResponse>(
        `${BACKEND_URL}/api/llm/generate`,
        {prompt, think},
    )

    return data
};
