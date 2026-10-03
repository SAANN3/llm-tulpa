import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface InstallTask {
    state: 'running' | 'done' | 'failed'
    phase: string
    completed_bytes: number
    total_bytes: number
    error: string | null
}

export interface Installed {
    tag: string
    build: string
    source: 'pinned' | 'latest' | 'custom'
    version: string | null
}

export interface InstallStatus {
    installed: Installed | null
    task: InstallTask | null
}

export type Channel = 'pinned' | 'latest' | 'custom'

/** What llama.cpp release is installed and how an install is going (owner only) */
export const getInstallStatus = async (): Promise<InstallStatus> => {
    const {data} = await axios.get<InstallStatus>(`${BACKEND_URL}/api/runtime/install`)
    return data
};

/** Starts an install in the background; watch it with `getInstallStatus` */
export const startInstall = async (channel: Channel, build?: string, customPath?: string): Promise<InstallTask> => {
    const {data} = await axios.post<InstallTask>(`${BACKEND_URL}/api/runtime/install`, {
        channel,
        build,
        custom_path: customPath,
    })
    return data
};
