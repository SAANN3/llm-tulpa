import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {GreetOut} from './greet'

export const blueprintName = async (content: string): Promise<string> => {
    const {data} = await axios.post<GreetOut>(`${BACKEND_URL}/api/prompts/blueprint_name`, {content})
    return data.response
};
