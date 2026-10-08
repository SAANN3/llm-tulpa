import {useRef, useState} from 'react'
import type {MouseEvent} from 'react'
import {MoreVertical} from 'pixelarticons/react'
import '../styles/chat-entry.scss'
import {Div, Label, Link} from './primitives'
import {ConfirmPopup} from './popups/base/confirm-popup.tsx'
import {ContextMenu} from './popups/base/context-menu.tsx'
import {InputPopup} from './popups/base/input-popup.tsx'

export interface ChatEntryProps {
    label: string
    selected: boolean
    /** The page the row opens, as a real link: a middle click or a long press opens it in a new tab */
    href?: string
    /** For a row that does something instead of opening a page (Log out) */
    onClicked?: () => void
    onRename?: (name: string) => void
    onDelete?: () => void
    /** Extra "Move to folder" menu item. Sibling of this file, `folder-entry.tsx`, is the
     * one used for a folder's own rows — a folder can't itself be assigned to a folder, so
     * that component has no equivalent prop rather than this one just going unused there. */
    onAssignFolder?: () => void
    /** Optional leading icon (JSX element) */
    icon?: React.ReactNode
}

/** One row in the chat list */
export const ChatEntry = ({label, selected, href, onClicked, onRename, onDelete, onAssignFolder, icon}: ChatEntryProps) => {
    const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(null)
    const [confirmingDelete, setConfirmingDelete] = useState(false)
    const [renaming, setRenaming] = useState(false)
    const triggerRef = useRef<HTMLDivElement>(null)
    const hasMenu = onRename != null && onDelete != null
    const showTrigger = hasMenu && selected

    const openAtTrigger = (e: MouseEvent<HTMLDivElement>) => {
        // Inside the row's link: neither the link's click nor the browser following it
        e.preventDefault()
        e.stopPropagation()
        const rect = triggerRef.current?.getBoundingClientRect()
        if (rect) setMenuPosition({x: rect.left, y: rect.bottom})
    }

    const openAtCursor = (e: MouseEvent<HTMLElement>) => {
        if (!hasMenu) return
        e.preventDefault()
        setMenuPosition({x: e.clientX, y: e.clientY})
    }

    const closeMenu = () => setMenuPosition(null)

    const content = (
        <>
            <Label text={label}/>
            {icon ? <Div className="chat-entry__icon" style={{ marginLeft: 'auto' }}>{icon}</Div> : null}
            {showTrigger ? (
                <Div ref={triggerRef} onClick={openAtTrigger} className="chat-entry__menu-trigger">
                    <MoreVertical width={16} height={16}/>
                </Div>
            ) : null}
        </>
    )

    return (
        <>
            {href != null ? (
                <Link to={href} onContextMenu={openAtCursor} variant={selected ? 'primary' : undefined}
                      className="list-row chat-entry">
                    {content}
                </Link>
            ) : (
                <Div onClick={onClicked} onContextMenu={openAtCursor} variant={selected ? 'primary' : undefined}
                     className="list-row chat-entry">
                    {content}
                </Div>
            )}

            {hasMenu ? (
                <ContextMenu
                    position={menuPosition}
                    onClose={closeMenu}
                    items={[
                        {label: 'Rename chat', onSelect: () => setRenaming(true)},
                        ...(onAssignFolder ? [{label: 'Move to folder', onSelect: onAssignFolder}] : []),
                        {label: 'Delete chat', onSelect: () => setConfirmingDelete(true), danger: true},
                    ]}
                />
            ) : null}

            {hasMenu ? (
                <ConfirmPopup
                    open={confirmingDelete}
                    title="Delete chat"
                    message={`Are you sure that you want to delete "${label}"`}
                    onConfirm={() => onDelete?.()}
                    onClose={() => setConfirmingDelete(false)}
                />
            ) : null}

            {hasMenu ? (
                <InputPopup
                    open={renaming}
                    title="Rename chat"
                    value={label}
                    onSubmit={(name) => onRename?.(name)}
                    onClose={() => setRenaming(false)}
                />
            ) : null}
        </>
    )
};
