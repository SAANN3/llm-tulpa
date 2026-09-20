export const MIN_TIMEZONE = -12
export const MAX_TIMEZONE = 14

export interface TimezoneValidation {
    valid: boolean
    message: string
    echo: string | null
}

/** Live validation for the raw timezone text field, parsed fresh on every keystroke */
export const validateTimezone = (text: string): TimezoneValidation => {
    const trimmed = text.trim()
    if (!trimmed) return {valid: false, message: 'Enter a UTC offset in whole hours.', echo: null}

    const n = Number(trimmed)
    if (!Number.isInteger(n)) return {valid: false, message: 'Whole hours only — 3, not 3.5.', echo: null}

    const echo = `UTC${n >= 0 ? '+' : ''}${n}`
    if (n < MIN_TIMEZONE || n > MAX_TIMEZONE) return {valid: false, message: 'Must be between UTC-12 and UTC+14.', echo}

    return {valid: true, message: "Detected automatically — change it if it's wrong.", echo}
};
