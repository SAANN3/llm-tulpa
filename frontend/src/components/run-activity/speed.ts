import type {SpeedPoint} from '../../hooks/use-speed-samples.ts'

/** The speed is worked out over this long, so a late report and the one after it even out */
const SPEED_WINDOW_MS = 1500
/** The graph reaches the top only when the speed passes this share above its average, so a steady reply sits below the top edge */
const HEADROOM = 1.3

/** The speed in tokens per second at each point, over the `SPEED_WINDOW_MS` before it */
const windowedSpeeds = (points: SpeedPoint[]): number[] => {
    const speeds: number[] = []
    let from = 0
    for (let i = 1; i < points.length; i++) {
        while (points[i].at - points[from].at > SPEED_WINDOW_MS) from++
        const ms = points[i].at - points[from].at
        speeds.push(ms > 0 ? ((points[i].tokens - points[from].tokens) / ms) * 1000 : 0)
    }
    return speeds
}

/** What a graph of the reply's speed is drawn from: the speeds, and the value that reaches the top (the scale starts at zero) */
export const speedSeries = (points: SpeedPoint[]): { values: number[]; max: number } => {
    const values = windowedSpeeds(points)
    const average = values.reduce((sum, speed) => sum + speed, 0) / Math.max(1, values.length)
    return {values, max: Math.max(1, HEADROOM * average, ...values)}
}
