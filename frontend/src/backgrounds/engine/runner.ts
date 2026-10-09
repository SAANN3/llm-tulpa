import type {BackgroundEffect} from '../index.ts'
import {sameLayout} from './page-layout.ts'
import {inkFor, Scene} from './scene.ts'
import type {Effect, PageLayout, Rgb} from './types.ts'

/** With the system's "reduce motion" on, everything moves at a quarter of its speed */
const REDUCED_MOTION_SPEED = 0.25

/** Reads a CSS color as RGB through the canvas, which accepts any CSS color syntax and hands back `#rrggbb` */
function parseColor(ctx: CanvasRenderingContext2D, value: string): Rgb {
    ctx.fillStyle = '#000'
    ctx.fillStyle = value
    const hex = String(ctx.fillStyle)
    if (!hex.startsWith('#')) return [0, 0, 0]
    return [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16)) as Rgb
}

/**
 * Runs one animated background on a canvas: loads the chosen effect, draws it at the effect's own frame rate, and
 * stops drawing while the tab is hidden. Everything it needs from the page (size, layout, theme colors, the user's
 * strength and speed) is handed to it; it changes nothing on the page.
 */
export class BackgroundRunner {
    private readonly scene: Scene
    private effect: Effect | null = null
    private fps = 15
    private speed = 1
    private frame = 0
    private last = 0
    private acc = 0
    private t = 0
    /** Bumped on every switch, so an effect whose code arrives after the next switch is dropped */
    private loading = 0
    private readonly reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)')

    constructor(canvas: HTMLCanvasElement) {
        const ctx = canvas.getContext('2d')
        if (!ctx) throw new Error('no 2d canvas')
        this.scene = new Scene(ctx)
        document.addEventListener('visibilitychange', this.onVisibility)
    }

    async setEffect(choice: BackgroundEffect): Promise<void> {
        const ticket = ++this.loading
        const factory = await choice.load()
        if (ticket !== this.loading) return
        this.effect = factory(this.scene)
        this.fps = choice.fps
        this.t = 0
        if (this.scene.cols) this.effect.init(true)
        this.start()
    }

    setTuning(strength: number, speed: number): void {
        this.scene.strength = strength
        this.speed = speed
    }

    setColors(style: CSSStyleDeclaration): void {
        const {ctx} = this.scene
        this.scene.colors = {
            bg: parseColor(ctx, style.getPropertyValue('--color-secondary').trim()),
            accent: parseColor(ctx, style.getPropertyValue('--color-tertiary').trim()),
            fg: parseColor(ctx, style.getPropertyValue('--color-primary').trim()),
        }
        this.scene.ink = inkFor(this.scene.colors.bg, this.scene.colors.accent)
    }

    /** A new layout re-fits the effect to it, keeping what it chose at random (the same tree, moved) */
    setLayout(layout: PageLayout): void {
        if (sameLayout(layout, this.scene.layout)) return
        this.scene.layout = layout
        if (this.scene.cols) this.effect?.init(false)
    }

    resize(width: number, height: number): void {
        this.scene.resize(width, height, window.devicePixelRatio || 1)
        this.effect?.init(false)
    }

    /** Stops drawing, until the next `setEffect` */
    stop(): void {
        this.loading++
        cancelAnimationFrame(this.frame)
        this.frame = 0
        this.effect = null
    }

    dispose(): void {
        this.stop()
        document.removeEventListener('visibilitychange', this.onVisibility)
    }

    private start(): void {
        if (this.frame || document.hidden || !this.effect) return
        this.last = performance.now()
        this.acc = 0
        this.frame = requestAnimationFrame(this.tick)
    }

    private readonly onVisibility = (): void => {
        if (document.hidden) {
            cancelAnimationFrame(this.frame)
            this.frame = 0
        } else this.start()
    }

    private readonly tick = (now: number): void => {
        this.frame = requestAnimationFrame(this.tick)
        // At most a quarter second at once: after a pause the scene carries on instead of jumping ahead
        this.acc += Math.min(.25, (now - this.last) / 1000)
        this.last = now
        if (this.acc < 1 / this.fps || !this.effect) return
        const d = this.acc * this.speed * (this.reducedMotion.matches ? REDUCED_MOTION_SPEED : 1)
        this.acc = 0
        this.t += d
        this.effect.draw(d, this.t)
    }
}
