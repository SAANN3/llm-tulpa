import type {Effect, EffectFactory} from '../engine/types.ts'
import {CloudsLayer, type CloudsOptions} from '../layers/clouds.ts'

/** A sky of clouds drifting across the window */
export const clouds = (o: CloudsOptions): EffectFactory => (s): Effect => {
    const layer = new CloudsLayer(s, o)
    return {
        init() { layer.init() },
        draw(d, t) { s.begin(); layer.draw(d, t); s.flush() },
    }
}
