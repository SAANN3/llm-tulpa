import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface ModelFolder {
    /** The model folder, null until one is chosen */
    path: string | null
    /** Whether files can be created in it, which downloads need */
    writable: boolean | null
    /** Registered models whose file is not in the folder */
    missing_models: string[]
    /** How many bytes can still be written on the disk the folder is on, when the system says */
    free_bytes: number | null
}

export interface BrowsedFolders {
    path: string
    parent: string | null
    folders: { name: string; path: string }[]
}

/** The folder the model files live in (owner only) */
export const getModelFolder = async (): Promise<ModelFolder> => {
    const {data} = await axios.get<ModelFolder>(`${BACKEND_URL}/api/runtime/folder`)
    return data
};

/** Chooses the model folder, effective at once (owner only) */
export const setModelFolder = async (path: string): Promise<ModelFolder> => {
    const {data} = await axios.post<ModelFolder>(`${BACKEND_URL}/api/runtime/folder`, {path})
    return data
};

/** The sub-folders of a folder, for choosing one (owner only) */
export const browseFolders = async (path?: string): Promise<BrowsedFolders> => {
    const {data} = await axios.get<BrowsedFolders>(`${BACKEND_URL}/api/runtime/folder/browse`, {params: path ? {path} : {}})
    return data
};
