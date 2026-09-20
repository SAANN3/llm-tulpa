import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {User} from '../auth/types'

/** Owner-only: creates a non-owner user */
export const createUser = async (username: string, password: string): Promise<User> => {
    const {data} = await axios.post<User>(`${BACKEND_URL}/api/users`, {username, password})
    return data
};
