import type {EffectFactory} from './engine/types.ts'

/** One animated background as the picker lists it. `load` fetches its code only when it is chosen. */
export interface BackgroundEffect {
    id: string
    name: string
    group: string
    /** Frames per second it is drawn at: calm scenes need fewer */
    fps: number
    description: string
    load: () => Promise<EffectFactory>
}

// The order is the picker's: by group, then as listed
export const backgroundEffects: readonly BackgroundEffect[] = [
    {id: 'ocean', group: 'Dots', name: 'Ocean swell', fps: 30, description: 'Three long waves of different lengths roll through the dots; they never line up.', load: () => import('./effects/dots.ts').then(m => m.dots({base: 26, tide: 74, grain: 62, flag: 0, flow: .65}))},
    {id: 'paper', group: 'Dots', name: 'Paper waves', fps: 30, description: 'Dot-grid paper with a print grain and one wave travelling through it.', load: () => import('./effects/dots.ts').then(m => m.dots({base: 26, tide: 0, grain: 73, flag: 23, flow: 1.0}))},

    {id: 'bonsai-grand', group: 'Bonsai', name: 'Grand bonsai (zen)', fps: 12, description: 'A big tree above the composer, about three quarters of the window high; two, one on each side, on centered pages.', load: () => import('./effects/bonsai.ts').then(m => m.bonsai({grand: true}))},
    {id: 'bonsai-grand-autumn', group: 'Bonsai', name: 'Grand bonsai (autumn)', fps: 12, description: 'The big tree in autumn colors, with leaves falling thickly.', load: () => import('./effects/bonsai.ts').then(m => m.bonsai({grand: true, autumn: true}))},
    {id: 'bonsai-grand-grows', group: 'Bonsai', name: 'Grand bonsai grows', fps: 12, description: 'The big tree grows from the pot up over about 80 seconds.', load: () => import('./effects/bonsai.ts').then(m => m.bonsai({grand: true, grow: true}))},
    {id: 'bonsai', group: 'Bonsai', name: 'Bonsai (zen)', fps: 12, description: 'A small tree.', load: () => import('./effects/bonsai.ts').then(m => m.bonsai({}))},
    {id: 'bonsai-autumn', group: 'Bonsai', name: 'Bonsai (autumn)', fps: 12, description: 'The small tree in autumn colors.', load: () => import('./effects/bonsai.ts').then(m => m.bonsai({autumn: true}))},
    {id: 'bonsai-grows', group: 'Bonsai', name: 'Bonsai grows', fps: 12, description: 'The small tree grows over about 40 seconds.', load: () => import('./effects/bonsai.ts').then(m => m.bonsai({grow: true}))},

    {id: 'fireflies', group: 'Night', name: 'Fireflies', fps: 15, description: 'Fireflies glowing up here and there.', load: () => import('./effects/night.ts').then(m => m.fireflies({}))},
    {id: 'starfield', group: 'Night', name: 'Starfield', fps: 15, description: 'Twinkling stars; a shooting star every 5 to 12 seconds.', load: () => import('./effects/night.ts').then(m => m.stars({gap: [5, 12]}))},
    {id: 'meteor-shower', group: 'Night', name: 'Meteor shower', fps: 15, description: 'The same sky in a meteor shower: one to three shooting stars every 1 to 4 seconds.', load: () => import('./effects/night.ts').then(m => m.stars({gap: [1.2, 4], multi: 3}))},
    {id: 'embers', group: 'Night', name: 'Embers', fps: 15, description: 'Embers rising from the bottom, dying out about halfway up.', load: () => import('./effects/night.ts').then(m => m.embers({}))},
    {id: 'embers-high', group: 'Night', name: 'Embers (rising high)', fps: 15, description: 'Embers rising higher and faster; some reach the top of the window.', load: () => import('./effects/night.ts').then(m => m.embers({travel: [.55, 1.0], v: [1.8, 4.0], count: (s) => Math.floor(s.cols * .75)}))},

    {id: 'sleeping-cat', group: 'Cozy scenes', name: 'Sleeping cat', fps: 15, description: 'Fireflies, and a cat asleep in the bottom right corner.', load: () => import('./effects/scenes.ts').then(m => m.scene({fireflies: true, props: ['sleepingCat']}))},
    {id: 'owl', group: 'Cozy scenes', name: 'Owl & cat night', fps: 15, description: 'Stars, the cat below and an owl on a branch above it (it blinks now and then).', load: () => import('./effects/scenes.ts').then(m => m.scene({stars: true, props: ['sleepingCat', 'owl']}))},
    {id: 'campfire', group: 'Cozy scenes', name: 'Campfire', fps: 15, description: 'A small campfire on the floor, sparks rising from it, stars above.', load: () => import('./effects/scenes.ts').then(m => m.scene({stars: true, props: ['campfire']}))},
    {id: 'cabin', group: 'Cozy scenes', name: 'Cabin in the night', fps: 15, description: 'A cabin with a lit window and smoke from the chimney, stars and fireflies.', load: () => import('./effects/scenes.ts').then(m => m.scene({stars: true, fireflies: true, props: ['cabin']}))},
    {id: 'night-train', group: 'Cozy scenes', name: 'Night train', fps: 15, description: 'Rails along the bottom and stars; every 35 to 75 seconds a train passes with lit windows and a trail of smoke.', load: () => import('./effects/scenes.ts').then(m => m.scene({stars: true, props: ['train']}))},
    {id: 'cup-of-tea', group: 'Cozy scenes', name: 'Cup of tea', fps: 15, description: 'A big cup of tea with a slow swirl on the surface and steam rising.', load: () => import('./effects/cup.ts').then(m => m.cup())},

    {id: 'soft-rain', group: 'Weather', name: 'Soft rain', fps: 15, description: 'Soft rain made of quiet characters (: . ·).', load: () => import('./effects/fall.ts').then(m => m.fall({v: [3.5, 7], wind: -0.12, wob: 0, density: .72, decay: 2.3, lv: [4, 3, 2], chars: [':', '.', '·']}))},
    {id: 'snow', group: 'Weather', name: 'Snow', fps: 15, description: 'Snowflakes swaying as they fall: large *, small +, a trail of dots.', load: () => import('./effects/fall.ts').then(m => m.fall({v: [0.8, 2.4], wind: 0, wob: 1.8, density: .55, decay: 5, lv: [4, 3, 2], chars: ['*', '+', '.']}))},
    {id: 'fog', group: 'Weather', name: 'Fog over the lake', fps: 12, description: 'Three layers of fog over a lake, a tree line in the distance, a boat with a warm lantern fading in and out.', load: () => import('./effects/fog.ts').then(m => m.fog())},
    {id: 'storm', group: 'Weather', name: 'Distant storm', fps: 12, description: 'Heavy clouds and soft rain; now and then a light quietly flares inside the clouds and fades.', load: () => import('./effects/storm.ts').then(m => m.storm())},

    {id: 'clouds-clear', group: 'Clouds', name: 'Clouds · clear', fps: 12, description: 'Three fluffy clouds at different depths, slowly changing shape.', load: () => import('./effects/clouds.ts').then(m => m.clouds({n: 3, kinds: ['cu'], z: [.3, 1], maxL: 4}))},
    {id: 'clouds-partly', group: 'Clouds', name: 'Clouds · partly', fps: 12, description: 'Heaps and long bands of cloud at different heights and speeds.', load: () => import('./effects/clouds.ts').then(m => m.clouds({n: 4, kinds: ['cu', 'cu', 'st'], z: [.15, 1], maxL: 4, y: (s) => [1, s.rows * .62]}))},
    {id: 'clouds-overcast', group: 'Clouds', name: 'Clouds · overcast', fps: 12, description: 'Wide flat layers of cloud, almost without contrast. Very calm.', load: () => import('./effects/clouds.ts').then(m => m.clouds({n: 8, kinds: ['st', 'st', 'cu'], z: [.2, .9], maxL: 3, big: 1.25, y: (s) => [0, s.rows * .62]}))},
    {id: 'clouds-rain', group: 'Clouds', name: 'Clouds · rain', fps: 12, description: 'The overcast sky with soft rain falling from it.', load: () => import('./effects/clouds.ts').then(m => m.clouds({n: 8, kinds: ['st', 'st', 'cu'], z: [.2, .9], maxL: 3, big: 1.25, rain: .3, y: (s) => [0, s.rows * .5]}))},

    {id: 'aquarium', group: 'Water', name: 'Aquarium', fps: 12, description: 'An aquarium filling the window: fish, weeds, a crab and bubbles.', load: () => import('./effects/water.ts').then(m => m.aquarium())},
    {id: 'deep-sea', group: 'Water', name: 'Deep sea (jellyfish)', fps: 15, description: 'Jellyfish slowly rising and pulsing, glowing plankton, bubbles. Quieter than the aquarium.', load: () => import('./effects/water.ts').then(m => m.deepSea())},

    {id: 'landscape-night', group: 'Landscapes', name: 'Night lake', fps: 12, description: 'The moon and its path on the water, three ridges in the haze, pines, a cabin with a warm window, a boat, fireflies on the shore, stars.', load: () => import('./effects/landscape.ts').then(m => m.landscape({night: true}))},
    {id: 'landscape-sunrise', group: 'Landscapes', name: 'Sunrise', fps: 12, description: 'The same landscape in the morning: the sun coming up behind the hills, clouds, a flock of birds over the lake.', load: () => import('./effects/landscape.ts').then(m => m.landscape({night: false}))},
    {id: 'landscape-winter', group: 'Landscapes', name: 'Winter night', fps: 12, description: 'Snowy pines, a cabin with smoke, a frozen lake that glints, slow snow and northern lights.', load: () => import('./effects/landscape.ts').then(m => m.landscape({night: true, winter: true}))},
    {id: 'rainy-city', group: 'Landscapes', name: 'Rainy evening (city)', fps: 15, description: 'Rain over a city: windows lighting up and going dark, a street lamp with a halo, ripples in puddles, drops sliding down the glass.', load: () => import('./effects/city.ts').then(m => m.rainyCity())},
    {id: 'field', group: 'Landscapes', name: 'Field & windmill', fps: 15, description: 'Wind running through the grass, a winding path, a windmill turning slowly on the hill, clouds and birds.', load: () => import('./effects/field.ts').then(m => m.field())},

    {id: 'lava', group: 'Abstract', name: 'Lava lamp', fps: 12, description: 'Big soft blobs of wax slowly rising, merging and sinking.', load: () => import('./effects/lava.ts').then(m => m.lava())},
]

/** What a browser shows behind the pages: nothing, today's static dots, or one of the animated backgrounds */
export type BackgroundId = 'off' | 'dots' | string

export interface BackgroundSettings {
    id: BackgroundId
    /** Brightness, 1 as tuned */
    strength: number
    /** Speed of the motion, 1 as tuned */
    speed: number
}

export const DEFAULT_BACKGROUND: BackgroundSettings = {id: 'dots', strength: 1, speed: 1}

export const findBackground = (id: BackgroundId): BackgroundEffect | undefined => backgroundEffects.find((e) => e.id === id)
