import {Div, Label} from '../../primitives'
import {Popup} from './popup.tsx'

export interface ContextMenuItem {
    label: string
    onSelect: () => void
    /** Highlighted, for an action that destroys something */
    danger?: boolean
}

export interface ContextMenuProps {
    /** Where the menu opens; `null` keeps it closed */
    position: { x: number; y: number } | null
    onClose: () => void
    items: ContextMenuItem[]
}

/** A small menu at a point, such as a row's right-click or "more" button. Picking an item
 * closes the menu first, then runs the item. */
export const ContextMenu = ({position, onClose, items}: ContextMenuProps) => (
    <Popup open={position != null} onClose={onClose} position={position ?? {x: 0, y: 0}}>
        {items.map((item) => (
            <Div
                key={item.label}
                variant={item.danger ? 'primary' : undefined}
                className="popup-menu__item"
                onClick={() => {
                    onClose()
                    item.onSelect()
                }}
            >
                <Label text={item.label}/>
            </Div>
        ))}
    </Popup>
);
