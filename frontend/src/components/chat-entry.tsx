import {useRef, useState} from 'react'
import type {MouseEvent} from 'react'
import {MoreVertical} from 'pixelarticons/react'
import '../styles/chat-entry.scss'
import {Button, Div, Input, Label} from './primitives'
import {Popup} from './popup.tsx'

export interface ChatEntryProps {
    label: string
    selected: boolean
    onClicked: () => void
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
export const ChatEntry = ({label, selected, onClicked, onRename, onDelete, onAssignFolder, icon}: ChatEntryProps) => {
    const [menuPosition, setMenuPosition] = useState<{ x: number; y: number } | null>(null)
    const [confirmingDelete, setConfirmingDelete] = useState(false)
    const [renaming, setRenaming] = useState(false)
    const [renameDraft, setRenameDraft] = useState(label)
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

    const startRename = () => {
        closeMenu()
        setRenameDraft(label)
        setRenaming(true)
    }

    const startDelete = () => {
        closeMenu()
        setConfirmingDelete(true)
    }

    return (
        <>
            <Div
                onClick={onClicked}
                onContextMenu={openAtCursor}
                variant={selected ? 'primary' : undefined}
                className="list-row chat-entry"
            >
                <Label text={label}/>
                {icon ? <Div className="chat-entry__icon" style={{ marginLeft: 'auto' }}>{icon}</Div> : null}
                {showTrigger ? (
                    <Div ref={triggerRef} onClick={openAtTrigger} className="chat-entry__menu-trigger">
                        <MoreVertical width={16} height={16}/>
                    </Div>
                ) : null}
            </Div>

            {hasMenu ? (
                <Popup open={menuPosition != null} onClose={closeMenu} position={menuPosition ?? {x: 0, y: 0}}>
                    <Div onClick={startRename} className="popup-menu__item">
                        <Label text="Rename chat"/>
                    </Div>
                    {onAssignFolder ? (
                        <Div
                            onClick={() => {
                                closeMenu()
                                onAssignFolder()
                            }}
                            className="popup-menu__item"
                        >
                            <Label text="Move to folder"/>
                        </Div>
                    ) : null}
                    <Div variant="primary" onClick={startDelete} className="popup-menu__item">
                        <Label text="Delete chat"/>
                    </Div>
                </Popup>
            ) : null}

            {hasMenu ? (
                <Popup open={confirmingDelete} onClose={() => setConfirmingDelete(false)} centered>
                    <Div className="vbox dialog">
                        <Label text={`Are you sure that you want to delete "${label}"`}/>
                        <Div className="dialog__actions">
                            <Button className="dialog__action" text="Cancel" variant="primary"
                                    onClicked={() => setConfirmingDelete(false)}/>
                            <Button
                                className="dialog__action"
                                text="Continue"
                                variant="secondary"
                                onClicked={() => {
                                    setConfirmingDelete(false)
                                    onDelete?.()
                                }}
                            />
                        </Div>
                    </Div>
                </Popup>
            ) : null}

            {hasMenu ? (
                <Popup open={renaming} onClose={() => setRenaming(false)} centered>
                    <Div className="vbox dialog">
                        <Input text={renameDraft} onChanged={setRenameDraft}/>
                        <Div className="dialog__actions">
                            <Button className="dialog__action" text="Cancel" variant="primary"
                                    onClicked={() => setRenaming(false)}/>
                            <Button
                                className="dialog__action"
                                text="Save"
                                variant="secondary"
                                onClicked={() => {
                                    setRenaming(false)
                                    onRename?.(renameDraft)
                                }}
                            />
                        </Div>
                    </Div>
                </Popup>
            ) : null}
        </>
    )
};
