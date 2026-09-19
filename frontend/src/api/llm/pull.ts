import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ModelTask} from './types'

/**
 * Starts pulling a model in the background (owner only) and returns the task to watch via
 * `listTasks`. `model` is a library name/tag, or `hf.co/<user>/<repo>[:<quant>]` for a GGUF on
 * Hugging Face.
 */
export const startPull = async (model: string): Promise<ModelTask> => {
    const {data} = await axios.post<ModelTask>(`${BACKEND_URL}/api/llm/pull`, {model})
    return data
};
