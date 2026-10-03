import axios from 'axios'
import {BACKEND_URL} from '../../config'

export const deletePreset = async (id: number): Promise<void> => {
    await axios.delete(`${BACKEND_URL}/api/presets`, {params: {id}})
};
