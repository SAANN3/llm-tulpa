import {useRef, useState} from 'react'
import type {MouseEvent} from 'react'
import {MoreVertical} from 'pixelarticons/react'
import '../styles/chat-entry.scss'
import {Div, Label} from './primitives'
import {ConfirmPopup} from './popups/base/confirm-popup.tsx'
import {ContextMenu} from './popups/base/context-menu.tsx'
import {InputPopup} from './popups/base/input-popup.tsx'

export interface FolderEntryProps {
    label: string
    selected: boolean
    onClicked: () => void
    onRename?: (name: string) => void
    onDelete?: () => void
}

/** One row in the folder list. Deliberately its own component rather than a parameterized
 * `ChatEntry` — a folder row has no "move to folder" concept and never will, so keeping it
 * separate means a future chat-only addition (e.g. pinning, unread state) can't leak into or
 * break folder rows just by being added to the shared component. */
export const FolderEntry = ({label, selected, onClicked, onRename, onDelete}: FolderEntryProps) => {
    const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(null)
    const [confirmingDelete, setConfirmingDelete] = useState(false)
    const [renaming, setRenaming] = useState(false)
    const triggerRef = useRef<HTMLDivElement>(null)
    const hasMenu = onRename != null && onDelete != null
    const showTrigger = hasMenu && selected

    const openAtTrigger = (e: MouseEvent<HTMLDivElement>) => {
        e.stopPropagation()
        const rect = triggerRef.current?.getBoundingClientRect()
        if (rect) setMenuPosition({x: rect.left, y: rect.bottom})
    }

    const openAtCursor = (e: MouseEvent<HTMLDivElement>) => {
        if (!hasMenu) return
        e.preventDefault()
        setMenuPosition({x: e.clientX, y: e.clientY})
    }

    const closeMenu = () => setMenuPosition(null)

    return (
        <>
            <Div
                onClick={onClicked}
                onContextMenu={openAtCursor}
                variant={selected ? 'primary' : undefined}
                className="list-row chat-entry"
            >
                <Label text={label}/>
                {showTrigger ? (
                    <Div ref={triggerRef} onClick={openAtTrigger} className="chat-entry__menu-trigger">
                        <MoreVertical width={16} height={16}/>
                    </Div>
                ) : null}
            </Div>

            {hasMenu ? (
                <ContextMenu
                    position={menuPosition}
                    onClose={closeMenu}
                    items={[
                        {label: 'Rename folder', onSelect: () => setRenaming(true)},
                        {label: 'Delete folder', onSelect: () => setConfirmingDelete(true), danger: true},
                    ]}
                />
            ) : null}

            {hasMenu ? (
                <ConfirmPopup
                    open={confirmingDelete}
                    title="Delete folder"
                    message={`Are you sure that you want to delete "${label}"`}
                    onConfirm={() => onDelete?.()}
                    onClose={() => setConfirmingDelete(false)}
                />
            ) : null}

            {hasMenu ? (
                <InputPopup
                    open={renaming}
                    title="Rename folder"
                    value={label}
                    onSubmit={(name) => onRename?.(name)}
                    onClose={() => setRenaming(false)}
                />
            ) : null}
        </>
    )
};
