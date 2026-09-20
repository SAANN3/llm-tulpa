import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {DatabaseForm} from './types'

/** Tests + persists the Postgres connection; rejects (400) if it can't connect */
export const setupDatabase = async (form: DatabaseForm): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/setup/database`, form)
};
