export type TimeFormat = '24h' | '12h'

/** What the browser's own locale uses, the default until the user picks one */
export const localeTimeFormat = (): TimeFormat => {
    const cycle = new Intl.DateTimeFormat(undefined, {hour: 'numeric'}).resolvedOptions().hourCycle
    return cycle === 'h11' || cycle === 'h12' ? '12h' : '24h'
}

/** A time of day with seconds, as `14:11:12` or `2:11:12 PM` */
export const formatTime = (date: Date, format: TimeFormat): string =>
    date.toLocaleTimeString(undefined, {hour12: format === '12h'})
