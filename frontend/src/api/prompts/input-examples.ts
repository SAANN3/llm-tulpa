import axios from 'axios'
import { BACKEND_URL } from '../../config'

export interface InputExampleOut {
  text: string
}

/** Fetches a placeholder example for the chat composer's empty input */
export const inputExample = async (signal?: AbortSignal): Promise<InputExampleOut> => {
  const { data } = await axios.post<InputExampleOut>(`${BACKEND_URL}/api/prompts/input_examples`, undefined, { signal })
  return data
};
