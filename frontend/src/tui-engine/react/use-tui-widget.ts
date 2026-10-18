import {useContext, useEffect, useState, type ReactNode} from 'react'
import type {TuiBaseProps} from '../core/types.ts'
import type {TuiWidget} from '../core/widget.ts'
import {ParentWidgetContext, useTuiScene} from './use-tui-scene.ts'

export interface TuiComponentProps extends TuiBaseProps {
    children?: ReactNode
}

/**
 * The life of a widget inside React: made once, handed the component's props after every render, added to its parent
 * (the enclosing box, else the canvas root) while mounted and asked to be redrawn whenever any of that changes.
 * Positions and sizes are not resolved here; the canvas lays the tree out before each frame, when every parent's size
 * is already known.
 */
export function useTuiWidget<T extends TuiWidget>(factory: () => T, props: TuiBaseProps, sync?: (widget: T) => void): T {
    const {rootWidget, requestRedraw} = useTuiScene()
    const parent = useContext(ParentWidgetContext) ?? rootWidget
    const [widget] = useState(factory)

    // No dependency list on purpose: the props are a new object each render and `sync` reads the latest ones
    useEffect(() => {
        widget.apply({...props, focusable: props.focusable ?? Boolean(props.onClick)})
        widget.clickable = Boolean(props.onClick)
        widget.onClick = props.onClick ?? (() => {})
        sync?.(widget)
        requestRedraw()
    })

    useEffect(() => {
        if (!parent) return
        parent.add(widget)
        requestRedraw()
        return () => {
            parent.remove(widget)
            requestRedraw()
        }
    }, [parent, widget, requestRedraw])

    return widget
}
