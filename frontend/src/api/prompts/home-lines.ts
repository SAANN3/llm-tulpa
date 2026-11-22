import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface HomeLines {
    greetings: string[]
    placeholders: string[]
}

/** The built-in greeting and placeholder lines, the same ones the home page picks from. */
export const homeLines = async (): Promise<HomeLines> => {
    const {data} = await axios.get<HomeLines>(`${BACKEND_URL}/api/prompts/home_lines`)
    return data
}
