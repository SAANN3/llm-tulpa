import axios from 'axios'
import {clearToken, getToken} from '../utils/auth-token'

/**
 * Configures the default axios instance
 */
export const installAuthInterceptors = () => {
    axios.interceptors.request.use((config) => {
        const token = getToken()
        if (token) config.headers.Authorization = `Bearer ${token}`
        return config
    })

    axios.interceptors.response.use(
        (response) => response,
        (error) => {
            const hadToken = Boolean(error.config?.headers?.Authorization)
            if (error.response?.status === 401 && hadToken) {
                clearToken()
                if (window.location.pathname !== '/login') window.location.assign('/login')
            }

            if (error.response?.status === 503 && window.location.pathname !== '/setup') {
                window.location.assign('/setup')
            }
            return Promise.reject(error)
        },
    )
};
