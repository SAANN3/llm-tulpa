import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface Hardware {
    os: string
    arch: string
    cpu: string
    cores: number
    ram_mib: number
    gpus: { vendor: string; name: string; vram_mib: number | null }[]
    /** The build id to start from: cpu, vulkan, rocm, cuda12, cuda13 */
    recommended: string
    reason: string
    builds: string[]
    /** `tested` on Linux, `untested` elsewhere */
    support: string
    missing: { name: string; hint: string }[]
}

/** What this machine has and which llama.cpp build suits it (owner only) */
export const getHardware = async (): Promise<Hardware> => {
    const {data} = await axios.get<Hardware>(`${BACKEND_URL}/api/runtime/hardware`)
    return data
};
