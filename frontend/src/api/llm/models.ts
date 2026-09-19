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

/** The models installed in the connected Ollama instance right now */
export const listModels = async (): Promise<LocalModel[]> => {
    const {data} = await axios.get<LocalModel[]>(`${BACKEND_URL}/api/llm/models`)
    return data
};
