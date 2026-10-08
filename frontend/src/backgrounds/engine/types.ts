import type {Scene} from './scene.ts'

/** A grid cell's brightness: 0 is empty, 1 to 4 are the accent at rising opacity, 5 is the text color */
export type Level = 0 | 1 | 2 | 3 | 4 | 5

export type Rgb = [number, number, number]

/** What the background needs to know about the page in front of it, measured from the real page */
export interface PageLayout {
    /** `chat`: a page with the sidebar (chats, home, search, folders); `center`: a centered panel and nothing else */
    kind: 'chat' | 'center'
    /** The sidebar's width in px, 0 on a centered page */
    sidebarPx: number
    /** The centered panel's width in px, on a centered page */
    contentPx: number
    /** Rows at the bottom hidden by the composer: scenes stand on the line above it */
    coveredRows: number
}

/** One background: built for a scene, then asked to draw a frame at the effect's own frame rate */
export interface Effect {
    /** Sets up for the scene's current size and layout. `reseed` false keeps what was chosen at random (the shape of
     * a tree) and only fits it to the new size */
    init(reseed: boolean): void
    /** `dt` is the seconds since the last frame and `t` the seconds since the start, both already scaled by speed */
    draw(dt: number, t: number): void
}

export type EffectFactory = (scene: Scene) => Effect
