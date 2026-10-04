import axios from 'axios'
import {BACKEND_URL} from '../../config'

export interface RemovedModel {
    /** How many chats were moved to another model */
    chats_moved: number
    /** How many sampling presets stay, now for any model */
    presets_kept: number
    /** Whether the file was deleted from the model folder */
    file_deleted: boolean
}

/** Removes a model (owner only), and with `deleteFile` its `.gguf` too; refused while a turn runs on it */
export const removeModel = async (id: number, deleteFile: boolean): Promise<RemovedModel> => {
    const {data} = await axios.delete<RemovedModel>(`${BACKEND_URL}/api/runtime/models`, {params: {id, delete_file: deleteFile}})
    return data
};
