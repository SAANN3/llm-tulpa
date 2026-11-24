import {useCallback, useEffect, useLayoutEffect, useRef, useState} from 'react'
import {ChevronLeft, ChevronRight} from 'pixelarticons/react'
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
    const viewsRef = useRef<HTMLDivElement>(null)
    const [overflowing, setOverflowing] = useState(false)
    const activeIndex = EDITOR_VIEWS.findIndex(item => item.id === view)
    // Arrows exist only while a section is off the strip, and they step to the next one the same way keys 1-9 do
    const canPrev = overflowing && activeIndex > 0
    const canNext = overflowing && activeIndex >= 0 && activeIndex < EDITOR_VIEWS.length - 1

    const checkOverflow = useCallback(() => {
        const strip = viewsRef.current
        if (!strip) return
        setOverflowing(strip.scrollWidth > strip.clientWidth + 2)
    }, [])

    useLayoutEffect(() => {
        checkOverflow()
        const strip = viewsRef.current
        if (!strip) return
        const observer = new ResizeObserver(checkOverflow)
        observer.observe(strip)
        return () => observer.disconnect()
    }, [checkOverflow])

    useEffect(() => {
        const strip = viewsRef.current
        const active = strip?.querySelector('.is-active')
        if (!strip || !(active instanceof HTMLElement) || !overflowing) return
        const left = active.offsetLeft - (strip.clientWidth / 2) + (active.offsetWidth / 2)
        strip.scrollTo({left: Math.max(0, left), behavior: 'smooth'})
    }, [view, overflowing, canPrev, canNext])

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
                </Div>
                <nav className="blueprint-editor__views" aria-label="Blueprint sections">
                    {canPrev ? (
                        <Button variant="secondary" className="blueprint-editor__views-arrow" title="Previous section"
                                onClicked={() => onView(EDITOR_VIEWS[activeIndex - 1].id)}>
                            <ChevronLeft width={16} height={16}/>
                        </Button>
                    ) : null}
                    <Div className="blueprint-editor__view-list" ref={viewsRef}>
                        {EDITOR_VIEWS.map((item, index) => (
                            <Button key={item.id} variant="secondary"
                                    className={view === item.id ? 'is-active' : ''}
                                    onClicked={() => onView(item.id)}>
                                <span>{index + 1}</span>
                                {item.label}
                            </Button>
                        ))}
                    </Div>
                    {canNext ? (
                        <Button variant="secondary" className="blueprint-editor__views-arrow" title="Next section"
                                onClicked={() => onView(EDITOR_VIEWS[activeIndex + 1].id)}>
                            <ChevronRight width={16} height={16}/>
                        </Button>
                    ) : null}
                </nav>
                <Button variant="secondary" className="blueprint-editor__exit" disabled={busy} text="Exit" onClicked={onBack}/>
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
