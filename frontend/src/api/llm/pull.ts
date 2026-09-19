import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Pulls a model into the connected Ollama instance; resolves once the pull finishes */
export const pullModel = async (model: string): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/llm/pull`, {model})
};
