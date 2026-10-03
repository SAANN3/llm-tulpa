import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface LocalModelDetails {
    family?: string | null
    parameter_size?: string | null
    quantization_level?: string | null
}

export interface LocalModel {
    name: string
    size?: number | null
    details?: LocalModelDetails | null
}

/** The models a provider offers right now: what is installed in Ollama, or the files registered to run on
 * the backend's own llama.cpp. Ollama unless another provider is named. */
export const listModels = async (provider = 'ollama'): Promise<LocalModel[]> => {
    const {data} = await axios.get<LocalModel[]>(`${BACKEND_URL}/api/llm/models`, {params: {provider}})
    return data
};
