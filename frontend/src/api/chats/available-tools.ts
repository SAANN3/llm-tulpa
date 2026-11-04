import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface AvailableTool {
    name: string
    /** The description the model is sent for this tool */
    description: string
}

/** Every tool this user can be sent */
export const getAvailableTools = async (): Promise<AvailableTool[]> => {
    const {data} = await axios.get<{tools: AvailableTool[]}>(`${BACKEND_URL}/api/chats/available_tools`)
    return data.tools
}
