import {useEffect, useRef, useState, type PointerEvent} from 'react'
import type {GraphPoint, NodeChoice} from '../../utils/blueprint-graph'

const DRAG_THRESHOLD = 6

export const useBlueprintPalette = (onAdd: (choice: NodeChoice, point?: GraphPoint) => void, onDrag: () => void, locked: boolean) => {
    const gesture = useRef<{choice: NodeChoice; pointer: number; start: GraphPoint; dragging: boolean} | null>(null)
    const suppressClick = useRef(false)
    const callbacks = useRef({onAdd, onDrag, locked})
    const [ghost, setGhost] = useState<{title: string; point: GraphPoint} | null>(null)

    useEffect(() => { callbacks.current = {onAdd, onDrag, locked} })
    useEffect(() => {
        const cancel = () => { gesture.current = null; setGhost(null) }
        const move = (event: globalThis.PointerEvent) => {
            const active = gesture.current
            if (!active || active.pointer !== event.pointerId) return
            if (callbacks.current.locked) { cancel(); return }
            if (!active.dragging && Math.hypot(event.clientX - active.start.x, event.clientY - active.start.y) < DRAG_THRESHOLD) return
            if (!active.dragging) {
                active.dragging = true
                suppressClick.current = true
                callbacks.current.onDrag()
            }
            setGhost({title: active.choice.definition.title, point: {x: event.clientX, y: event.clientY}})
        }
        const up = (event: globalThis.PointerEvent) => {
            const active = gesture.current
            if (!active || active.pointer !== event.pointerId) return
            if (active.dragging && !callbacks.current.locked) callbacks.current.onAdd(active.choice, {x: event.clientX, y: event.clientY})
            cancel()
        }
        const key = (event: KeyboardEvent) => { if (event.key === 'Escape') cancel() }
        window.addEventListener('pointermove', move)
        window.addEventListener('pointerup', up)
        window.addEventListener('pointercancel', cancel)
        window.addEventListener('blur', cancel)
        window.addEventListener('keydown', key)
        return () => {
            window.removeEventListener('pointermove', move)
            window.removeEventListener('pointerup', up)
            window.removeEventListener('pointercancel', cancel)
            window.removeEventListener('blur', cancel)
            window.removeEventListener('keydown', key)
        }
    }, [])

    const start = (event: PointerEvent, choice: NodeChoice) => {
        if (event.button !== 0 || locked) return
        suppressClick.current = false
        gesture.current = {choice, pointer: event.pointerId, start: {x: event.clientX, y: event.clientY}, dragging: false}
    }
    const click = (choice: NodeChoice, detail: number) => {
        if (locked || (suppressClick.current && detail !== 0)) return
        onAdd(choice)
    }
    return {ghost, start, click}
}
