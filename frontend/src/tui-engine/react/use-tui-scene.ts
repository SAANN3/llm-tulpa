import {createContext, useContext} from 'react'
import type {TuiScene} from '../core/scene.ts'
import type {TuiWidget} from '../core/widget.ts'

export interface TuiSceneContextValue {
    /** Null until the canvas has been created */
    scene: TuiScene | null
    /** The widget tree's root, which widgets outside any box are added to; null outside a canvas */
    rootWidget: TuiWidget | null
    /** The whole cells that fit the canvas, and its size in px */
    cols: number
    rows: number
    width: number
    height: number
    /** Asks for a frame, for a canvas that draws only when something changed */
    requestRedraw: () => void
}

export const TuiSceneContext = createContext<TuiSceneContextValue>({
    scene: null,
    rootWidget: null,
    cols: 0,
    rows: 0,
    width: 0,
    height: 0,
    requestRedraw: () => {},
})

/** The canvas a component is inside: its scene, its size and how to ask for a frame */
export const useTuiScene = (): TuiSceneContextValue => useContext(TuiSceneContext)

/** The box (or other container widget) a component is inside; widgets add themselves to it */
export const ParentWidgetContext = createContext<TuiWidget | null>(null)
