import axios from 'axios'
import {clearToken, getToken} from '../utils/auth-token'

/**
 * What the app does when the backend says the session is gone or the install isn't usable. Set
 * by the providers that own that state (`AuthProvider`, `SetupProvider`) — this module has no
 * router or React state of its own, and the route guards turn the state change into a
 * `<Navigate>`, so nothing here reloads the page.
 */
interface ClientHandlers {
    onUnauthorized?: () => void
    onSetupRequired?: () => void
}

const handlers: ClientHandlers = {}

/** Registers (or, with `undefined`, removes) a handler; the returned cleanup removes it */
export const setClientHandler = <K extends keyof ClientHandlers>(key: K, handler: ClientHandlers[K]) => {
    handlers[key] = handler
    return () => {
        if (handlers[key] === handler) delete handlers[key]
    }
};

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
                handlers.onUnauthorized?.()
            }

            if (error.response?.status === 503) handlers.onSetupRequired?.()
            return Promise.reject(error)
        },
    )
};
