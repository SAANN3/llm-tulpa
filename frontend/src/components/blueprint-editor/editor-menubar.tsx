import {useState} from 'react'
import {ArrowLeft} from 'pixelarticons/react'
import {Button, Div} from '../primitives'
import {ContextMenu, type ContextMenuItem} from '../popups/base/context-menu'
import type {KeyHandler} from '../../hooks/use-key-layer'
import {EDITOR_VIEWS} from '../../utils/blueprint-views'

type MenuId = 'file' | 'edit' | 'view'

interface EditorMenubarProps {
    view: string
    busy: boolean
    menus: Record<MenuId, ContextMenuItem[]>
    onView: (view: typeof EDITOR_VIEWS[number]['id']) => void
    onBack: () => void
    onKeyDown: KeyHandler
}

export const EditorMenubar = ({view, busy, menus, onView, onBack, onKeyDown}: EditorMenubarProps) => {
    const [menu, setMenu] = useState<{id: MenuId; x: number; y: number} | null>(null)

    return (
        <>
            <header className="blueprint-editor__menubar">
                <Div className="blueprint-editor__menus">
                    {(['file', 'edit', 'view'] as const).map(key => (
                        <Button key={key} variant="secondary"
                                text={{file: 'File', edit: 'Edit', view: 'View'}[key]}
                                onClicked={event => {
                                    const rect = event.currentTarget.getBoundingClientRect()
                                    setMenu(menu?.id === key ? null : {id: key, x: rect.left, y: rect.bottom + 3})
                                }}/>
                    ))}
                    <Button variant="secondary" disabled={busy} onClicked={onBack}>
                        <ArrowLeft width={16} height={16}/> Back
                    </Button>
                </Div>
                <nav className="blueprint-editor__views" aria-label="Blueprint sections">
                    {EDITOR_VIEWS.map((item, index) => (
                        <Button key={item.id} variant="secondary"
                                className={view === item.id ? 'is-active' : ''}
                                onClicked={() => onView(item.id)}>
                            <span>{index + 1}</span>
                            {item.label}
                        </Button>
                    ))}
                </nav>
            </header>
            <ContextMenu position={menu ? {x: menu.x, y: menu.y} : null}
                         onClose={() => setMenu(null)} items={menu ? menus[menu.id] : []}
                         onKeyDown={event => {
                             const handled = onKeyDown(event)
                             if (handled) setMenu(null)
                             return handled
                         }}/>
        </>
    )
}
