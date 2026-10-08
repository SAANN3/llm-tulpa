import {noise, rand} from '../engine/random.ts'
import {level} from '../engine/scene.ts'
import type {Effect, EffectFactory} from '../engine/types.ts'
import {CloudsLayer} from '../layers/clouds.ts'
import {StaticLayer} from '../layers/static-layer.ts'

/** A flash's brightness over time: a quick rise, then a slow fade */
const pulse = (x: number): number => x < 0 ? 0 : x < .08 ? x / .08 : Math.exp(-(x - .08) / .3)

/** A distant storm: heavy clouds and soft rain; every 5 to 14 seconds a light quietly flares inside the clouds and
 * slowly fades (no lightning, nothing sharp) */
export const storm = (): EffectFactory => (s): Effect => {
    const fixed = new StaticLayer(s)
    let clouds: CloudsLayer | null = null, vis = 0, flash: { t0: number; cx: number; cy: number; R: number } | null = null, next = 4, seed = 0
    return {
        init() {
            vis = s.visibleRows(); seed = rand() * 300; fixed.reset()
            for (let c = 0; c < s.cols; c++) {
                const r0 = Math.round(vis * (.8 + (noise(c * .04 + seed, 2, 2) - .5) * .06))
                for (let r = r0; r < s.rows; r++) fixed.put(r, c, r === r0 ? '_' : '.', r === r0 ? 2 : 1)
            }
            clouds = new CloudsLayer(s, {n: 7, kinds: ['st', 'st', 'cu'], z: [.2, .9], maxL: 3, big: 1.4, rain: .26, y: (sc) => [0, sc.rows * .5]})
            clouds.init(); flash = null; next = 3 + rand() * 4
        },
        draw(d, t) {
            const {cols, rows} = s
            s.begin()
            fixed.blit(); clouds?.draw(d, t)
            if (!flash && t >= next) flash = {t0: t, cx: cols * (.15 + rand() * .7), cy: rows * (.08 + rand() * .3), R: 20 + rand() * 20}
            if (flash) {
                const dt = t - flash.t0, env = Math.max(pulse(dt), .7 * pulse(dt - .32))
                if (dt > 1.8) { flash = null; next = t + 5 + rand() * 9 }
                else if (env > .05) {
                    const r0 = Math.max(0, Math.floor(flash.cy - flash.R * .5)), r1 = Math.min(rows - 1, Math.ceil(flash.cy + flash.R * .5))
                    for (let r = r0; r <= r1; r++) for (let c = Math.max(0, Math.floor(flash.cx - flash.R)); c < Math.min(cols, Math.ceil(flash.cx + flash.R)); c++) {
                        const dd = ((c - flash.cx) / flash.R) ** 2 + ((r - flash.cy) / (flash.R * .5)) ** 2
                        if (dd >= 1) continue
                        const i = r * cols + c
                        if (s.levels[i] === 0) continue
                        s.levels[i] = level(Math.min(4, s.levels[i] + Math.round(env * (1 - dd) * 2.2)))
                    }
                }
            }
            s.flush()
        },
    }
}
