import {useSyncExternalStore} from 'react'
import {getDebugLines, isDebug, subscribeDebug, subscribeDebugLines} from '../utils/debug.ts'

/** Whether this browser's debug switch is on, for something that shows diagnostics on screen */
export const useDebug = (): boolean => useSyncExternalStore(subscribeDebug, isDebug)

/** The newest debug lines, oldest first; empty while the switch is off */
export const useDebugLines = (): readonly string[] => useSyncExternalStore(subscribeDebugLines, getDebugLines)
