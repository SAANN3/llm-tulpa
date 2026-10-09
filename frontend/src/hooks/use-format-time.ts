import {useCallback} from 'react'
import {useTheme} from '../context/use-theme.ts'
import {formatTime} from '../utils/time-format.ts'

/** Formats a time of day the way the user picked in Settings (24-hour or 12-hour) */
export const useFormatTime = () => {
    const {timeFormat} = useTheme()
    return useCallback((when: string | Date) => formatTime(new Date(when), timeFormat), [timeFormat])
};
