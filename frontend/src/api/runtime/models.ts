import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {ManagedModel} from './types'

/** The model files registered to run on the backend's own llama.cpp */
export const listManagedModels = async (): Promise<ManagedModel[]> => {
    const {data} = await axios.get<{ models: ManagedModel[] }>(`${BACKEND_URL}/api/runtime/models`)
    return data.models
};
