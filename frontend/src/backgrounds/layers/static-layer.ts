import type {Level} from '../engine/types.ts'
import type {Scene} from '../engine/scene.ts'

/** A scene's fixed part (buildings, hills, a path): built once in `init`, copied onto the grid every frame */
export class StaticLayer {
    private readonly s: Scene
    private chars: string[] = []
    private levels: Uint8Array = new Uint8Array(0)

    constructor(s: Scene) {
        this.s = s
    }

    reset(): void {
        this.chars = new Array<string>(this.s.cols * this.s.rows).fill(' ')
        this.levels = new Uint8Array(this.s.cols * this.s.rows)
    }

    put(r: number, c: number, ch: string, l: Level): void {
        if (!this.s.inside(r, c)) return
        const i = r * this.s.cols + c
        this.chars[i] = ch
        this.levels[i] = l
    }

    has(r: number, c: number): boolean {
        return this.s.inside(r, c) && this.levels[r * this.s.cols + c] > 0
    }

    blit(): void {
        const {chars, levels} = this.s
        for (let i = 0; i < this.levels.length; i++) if (this.levels[i]) { chars[i] = this.chars[i]; levels[i] = this.levels[i] }
    }
}
