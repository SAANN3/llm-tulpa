import { useRef, useState } from 'react'
import type { MouseEvent } from 'react'
import { MoreVertical } from 'pixelarticons/react'

import '../styles/ChatEntry.scss'
import { Button, Div, Input, Label } from './primitives'
import { Popup } from './Popup'

export interface ChatEntryProps {
  label: string
  selected: boolean
  onClicked: () => void
  /** When both given, the row gets a three-dot action button plus right-click support, both opening a rename/delete popup positioned at whichever triggered it. Omitted for non-chat uses of this row (e.g. the sidebar's Settings entry), which get a plain row with no menu. */
  onRename?: (name: string) => void
  onDelete?: () => void
}

/** One row in the chat list — also doubles as a generic selectable row for any future tab/list, so no separate RowEntry. */
export function ChatEntry({ label, selected, onClicked, onRename, onDelete }: ChatEntryProps) {
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
    if (rect) setMenuPosition({ x: rect.left, y: rect.bottom })
  }

  const openAtCursor = (e: MouseEvent<HTMLDivElement>) => {
    if (!hasMenu) return
    e.preventDefault()
    setMenuPosition({ x: e.clientX, y: e.clientY })
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
        <Label text={label} />
        {showTrigger ? (
          <Div ref={triggerRef} onClick={openAtTrigger} className="chat-entry__menu-trigger">
            <MoreVertical width={16} height={16} />
          </Div>
        ) : null}
      </Div>

      {hasMenu ? (
        <Popup open={menuPosition != null} onClose={closeMenu} position={menuPosition ?? { x: 0, y: 0 }}>
          <Div onClick={startRename} className="popup-menu__item">
            <Label text="Rename chat" />
          </Div>
          <Div variant="primary" onClick={startDelete} className="popup-menu__item">
            <Label text="Delete chat" />
          </Div>
        </Popup>
      ) : null}

      {hasMenu ? (
        <Popup open={confirmingDelete} onClose={() => setConfirmingDelete(false)} centered>
          <Div className="vbox dialog">
            <Label text={`Are you sure that you want to delete "${label}"`} />
            <Div className="dialog__actions">
              <Button className="dialog__action" text="Cancel" variant="primary" onClicked={() => setConfirmingDelete(false)} />
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
            <Input text={renameDraft} onChanged={setRenameDraft} />
            <Div className="dialog__actions">
              <Button className="dialog__action" text="Cancel" variant="primary" onClicked={() => setRenaming(false)} />
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
}
