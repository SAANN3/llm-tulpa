import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface OllamaSettings {
    /** Where Ollama answers */
    url: string
    /** The context window Ollama runs the model with; applies from the next start */
    context_length: number
}

export interface OllamaTest {
    reachable: boolean
    /** How many models are installed, when it answered */
    models: number | null
    /** Why it didn't answer */
    reason: string | null
}

/** Where the backend reaches Ollama, and the context window it assumes (owner only) */
export const getOllamaSettings = async (): Promise<OllamaSettings> => {
    const {data} = await axios.get<OllamaSettings>(`${BACKEND_URL}/api/llm/ollama`)
    return data
};

/** Changes them; the address applies at once (owner only) */
export const setOllamaSettings = async (settings: OllamaSettings): Promise<OllamaSettings> => {
    const {data} = await axios.post<OllamaSettings>(`${BACKEND_URL}/api/llm/ollama`, settings)
    return data
};

/** Asks the Ollama at an address for its models, without saving anything (owner only) */
export const testOllama = async (url: string): Promise<OllamaTest> => {
    const {data} = await axios.post<OllamaTest>(`${BACKEND_URL}/api/llm/ollama/test`, {url})
    return data
};
