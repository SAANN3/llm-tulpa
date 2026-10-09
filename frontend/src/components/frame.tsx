import type {CSSProperties, ReactNode} from 'react'
import '../styles/frame.scss'
import {isTextEntry, keyBelongsToFocus, useKeyLayer} from '../hooks/use-key-layer.ts'
import {Button, Div} from './primitives'

/** One of a frame's tabs; its place in the list is its number and its key */
export interface FrameTab {
    id: string
    label: string
}

/** Something a frame does on a key, shown in its footer. The footer is built only from these, so it lists what the
 * keyboard really does there: an action without `run` is shown as a hint only, for something the keys can't do on
 * their own (a drag, a double-click). */
export interface FrameAction {
    /** The `KeyboardEvent.key` values that trigger it */
    keys: string[]
    /** What the footer says the action does */
    label: string
    /** How the footer shows the keys, when `keys` joined by a space doesn't read well (`← →`) */
    shown?: string
    run?: (key: string) => void
    /** Also runs while the user is typing in a field; for keys that don't type anything (Escape) */
    whileTyping?: boolean
}

export interface FrameProps {
    /** The label on the frame's top edge; a frame with tabs shows the tabs there instead */
    title?: string
    /** Tabs on the top edge, numbered in this order: keys 1 to 9 switch to them */
    tabs?: FrameTab[]
    activeTab?: string
    onTab?: (id: string) => void
    /** What the frame's own keys do beyond the tab keys and `onEscape`, which it adds itself. Required, so a new
     * frame says what it offers; an empty list says there is nothing more. */
    actions: FrameAction[]
    /** Escape: closes a popup, leaves a page */
    onEscape?: () => void
    /** How the footer names `onEscape` */
    escapeLabel?: 'close' | 'back'
    /** Whether this frame has the keyboard: a page's frame always, a popup's while it is open */
    active?: boolean
    className?: string
    boxClassName?: string
    bodyClassName?: string
    style?: CSSProperties
    children: ReactNode
}

const keysText = (action: FrameAction) => action.shown ?? action.keys.join(' ')

/** The app's panel: a double frame with a title or numbered tabs on its top edge and, along its bottom, a line of the
 * keys that work in it. Every page's panel and every dialog is one, so they look and handle keys alike; the key
 * handling joins the shared layer stack, where only the newest frame (an open popup over a page) hears a key. */
export const Frame = ({
    title,
    tabs,
    activeTab,
    onTab,
    actions,
    onEscape,
    escapeLabel = 'close',
    active = true,
    className,
    boxClassName,
    bodyClassName,
    style,
    children,
}: FrameProps) => {
    const tabActions: FrameAction[] = tabs && tabs.length > 1 && onTab
        ? [{
            keys: tabs.slice(0, 9).map((_, i) => String(i + 1)),
            shown: `1-${Math.min(tabs.length, 9)}`,
            label: 'tab',
            run: (key) => onTab(tabs[Number(key) - 1].id),
        }]
        : []
    const escapeActions: FrameAction[] = onEscape
        ? [{keys: ['Escape'], shown: 'esc', label: escapeLabel, run: onEscape, whileTyping: escapeLabel === 'close'}]
        : []
    const all = [...tabActions, ...actions, ...escapeActions]

    useKeyLayer(active, (e) => {
        if (e.ctrlKey || e.metaKey || e.altKey) return false
        const action = all.find((a) => a.run && a.keys.includes(e.key))
        // Escape in a text field leaves the field, so the frame's keys work again; a second one is the frame's
        if (e.key === 'Escape' && !action?.whileTyping && isTextEntry(e.target as Element)) {
            (e.target as HTMLElement).blur()
            return true
        }
        if (!action?.run || (keyBelongsToFocus(e) && !action.whileTyping)) return false
        action.run(e.key)
        return true
    })

    // The escape hint sits at the far end of the line, on its own: it leaves the frame, the rest act inside it
    const inside = [...tabActions, ...actions]

    return (
        <Div className={['frame', tabs ? 'frame--tabbed' : null, className].filter(Boolean).join(' ')} style={style}>
            {tabs ? (
                <Div className="frame__tabs">
                    {tabs.map((tab, i) => (
                        <Button key={tab.id} variant="secondary"
                                className={`frame__tab${tab.id === activeTab ? ' frame__tab--on' : ''}`}
                                onClicked={() => onTab?.(tab.id)}>
                            {i < 9 ? <span className="frame__tab-key">{i + 1}</span> : null}
                            {tab.label}
                        </Button>
                    ))}
                </Div>
            ) : null}
            <Div className={['dos-frame', 'frame__box', boxClassName].filter(Boolean).join(' ')}>
                {title && !tabs ? <span className="dos-frame__title">{title}</span> : null}
                <Div className={['dos-frame__body', 'frame__body', bodyClassName].filter(Boolean).join(' ')}>{children}</Div>
                {inside.length > 0 || escapeActions.length > 0 ? (
                    <Div className="frame__hints">
                        {inside.map((action) => (
                            <span key={action.label} className="frame__hint">
                                <span className="frame__hint-keys">{keysText(action)}</span> {action.label}
                            </span>
                        ))}
                        <span className="frame__hints-gap"/>
                        {escapeActions.map((action) => (
                            <span key={action.label} className="frame__hint">
                                <span className="frame__hint-keys">{keysText(action)}</span> {action.label}
                            </span>
                        ))}
                    </Div>
                ) : null}
            </Div>
        </Div>
    )
};

