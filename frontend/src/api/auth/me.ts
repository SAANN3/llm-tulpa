import axios from 'axios'
import {BACKEND_URL} from '../../config'
import type {User} from './types'

/** The currently-authenticated user, from the stored token */
export const me = async (): Promise<User> => {
    const {data} = await axios.get<User>(`${BACKEND_URL}/api/auth/me`)
    return data
};
