import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** The level a model starts on in a chat that has none of its own: `off`, `on` or one of the model's own levels */
export interface ThinkingDefault {
    provider: string
    model: string
    level: string
}

/** Every model the user has chosen a starting level for */
export const getThinkingDefaults = async (): Promise<ThinkingDefault[]> => {
    const {data} = await axios.get<ThinkingDefault[]>(`${BACKEND_URL}/api/llm/thinking_defaults`)
    return data
};

/** Chooses the level a model starts on, or (`level` null) takes the choice away */
export const setThinkingDefault = async (provider: string, model: string, level: string | null): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/llm/thinking_default`, {provider, model, level})
};
