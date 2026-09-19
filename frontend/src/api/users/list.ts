import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {User} from '../auth/types'

/** Owner-only: every account, oldest first */
export const listUsers = async (): Promise<User[]> => {
    const {data} = await axios.get<User[]>(`${BACKEND_URL}/api/users`)
    return data
};
