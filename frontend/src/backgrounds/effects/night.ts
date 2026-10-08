import type {Effect, EffectFactory} from '../engine/types.ts'
import {EmbersLayer, type EmbersOptions} from '../layers/embers.ts'
import {FirefliesLayer, type FirefliesOptions} from '../layers/fireflies.ts'
import {StarsLayer, type StarsOptions} from '../layers/stars.ts'

/** Fireflies on the dark field */
export const fireflies = (o: FirefliesOptions = {}): EffectFactory => (s): Effect => {
    const layer = new FirefliesLayer(s, o)
    return {
        init() { layer.init() },
        draw(d, t) { s.begin(); layer.put(d, t); s.flush() },
    }
}

/** Twinkling stars and shooting stars */
export const stars = (o: StarsOptions = {}): EffectFactory => (s): Effect => {
    const layer = new StarsLayer(s, o)
    return {
        init() { layer.init() },
        draw(d, t) { s.begin(); layer.draw(d, t); s.flush() },
    }
}

/** Sparks rising from the bottom edge */
export const embers = (o: EmbersOptions = {}): EffectFactory => (s): Effect => {
    const layer = new EmbersLayer(s, o)
    return {
        init() { layer.init() },
        draw(d) { s.begin(); layer.put(d); s.flush() },
    }
}
