import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface GpuLoad {
    vram_used_bytes: number
    vram_total_bytes: number
    busy_percent: number | null
    temperature_c: number | null
    clock_mhz: number | null
}

/** What the machine is using right now */
export interface SystemSnapshot {
    /** All cores together, since the previous call */
    cpu_percent: number
    cpu_threads: number
    memory_used_bytes: number
    memory_total_bytes: number
    /** The model server process's own memory, when one runs */
    server_memory_bytes: number | null
    /** The AMD GPU's live figures (Linux); null elsewhere */
    gpu: GpuLoad | null
}

/** A snapshot of the machine's load; CPU use is measured since the previous call, so poll it */
export const getSystemSnapshot = async (): Promise<SystemSnapshot> => {
    const {data} = await axios.get<SystemSnapshot>(`${BACKEND_URL}/api/runtime/system`)
    return data
};
