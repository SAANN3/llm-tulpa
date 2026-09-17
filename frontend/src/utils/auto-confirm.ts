const STORAGE_KEY = 'llm-tulpa:auto_confirm'

/** Auto-confirm mode is a local per-browser preference, not synced through Settings */

export const getAutoConfirm = (): boolean => {
    try {
        return localStorage.getItem(STORAGE_KEY) === 'true'
    } catch {
        return false
    }
};

export const setAutoConfirm = (enabled: boolean): void => {
    try {
        localStorage.setItem(STORAGE_KEY, String(enabled))
    } catch {
    }
};
