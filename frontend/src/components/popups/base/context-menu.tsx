import type {ReactNode} from 'react'
import {Div, Label, ToggleSwitch} from '../../primitives'
import {Popup} from './popup.tsx'
import type {KeyHandler} from '../../../hooks/use-key-layer.ts'

export interface ContextMenuItem {
    label: string
    onSelect: () => void
    /** An action that destroys something: kept apart at the end of the menu, in the danger color */
    danger?: boolean
    /** Shown before the label */
    icon?: ReactNode
    /** Makes the item an on/off switch showing this state; picking it flips the switch and leaves the menu open, so
     * the change is seen */
    toggled?: boolean
    disabled?: boolean
    shortcut?: string
}

export interface ContextMenuProps {
    /** Where the menu opens; `null` keeps it closed */
    position: { x: number; y: number } | null
    onClose: () => void
    items: ContextMenuItem[]
    /** See `Popup`'s menu `minWidth` */
    minWidth?: number
    /** See `Popup`'s menu `corner` */
    corner?: 'top-left' | 'bottom-right' | 'bottom-left'
    onKeyDown?: KeyHandler
}

/** A small menu at a point, such as a row's right-click or a "more" button. Picking an item closes the menu first,
 * then runs the item (a switch item stays open). */
export const ContextMenu = ({position, onClose, items, minWidth, corner, onKeyDown}: ContextMenuProps) => (
    <Popup open={position != null} onClose={onClose} position={position ?? {x: 0, y: 0}} minWidth={minWidth} corner={corner} onKeyDown={onKeyDown}>
        {items.map((item) => (
            <Div
                key={item.label}
                className={['popup-menu__item', item.icon ? 'popup-menu__item--icon' : null, item.toggled != null || item.shortcut ? 'popup-menu__item--trailing' : null, item.danger ? 'popup-menu__item--danger' : null, item.disabled ? 'popup-menu__item--disabled' : null].filter(Boolean).join(' ')}
                onClick={() => {
                    if (item.disabled) return
                    if (item.toggled == null) onClose()
                    item.onSelect()
                }}
            >
                {item.icon ?? null}
                <Label text={item.label}/>
                {item.shortcut ? <kbd className="popup-menu__shortcut">[{item.shortcut}]</kbd> : null}
                {item.toggled != null ? (
                    // The row's click flips it; the switch only shows the state
                    <ToggleSwitch toggled={item.toggled} disabled={item.disabled} onToggled={() => undefined}/>
                ) : null}
            </Div>
        ))}
    </Popup>
);
