import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface ServerSettings {
    /** Stop the server after this many idle minutes, freeing the GPU; 0 never stops it */
    idle_unload_minutes: number
    /** Load the default model when the backend starts (applies from the next start) */
    autostart: boolean
    /** How long to wait for a model to finish loading, in seconds */
    load_timeout_secs: number
}

/** How the model server behaves over time (owner only) */
export const getServerSettings = async (): Promise<ServerSettings> => {
    const {data} = await axios.get<ServerSettings>(`${BACKEND_URL}/api/runtime/server`)
    return data
};

/** Changes them; the idle time and the load timeout apply at once (owner only) */
export const setServerSettings = async (settings: ServerSettings): Promise<ServerSettings> => {
    const {data} = await axios.post<ServerSettings>(`${BACKEND_URL}/api/runtime/server`, settings)
    return data
};
