import {useState} from 'react'
import {Analytics, ChevronUp, Cpu, Logout, Plug, Settings2, User} from 'pixelarticons/react'
import {useNavigate} from 'react-router-dom'
import {ContextMenu, type ContextMenuItem} from './popups/base/context-menu.tsx'
import {ConfirmPopup} from './popups/base/confirm-popup.tsx'
import {Button, Div} from './primitives'
import {useAuth} from '../context/use-auth.ts'

/** The account at the bottom of the sidebar: who is signed in, and a menu of the pages that aren't used every day
 * (Settings, Usage, Models, and for the owner Plugins and Users) and Log out, which asks first. In the shrunk rail
 * it is an icon. */
export const AccountMenu = ({compact = false}: { compact?: boolean }) => {
    const navigate = useNavigate()
    const {user, logout} = useAuth()
    const [menuAt, setMenuAt] = useState<{ x: number; y: number; width: number } | null>(null)
    const [confirmingLogout, setConfirmingLogout] = useState(false)

    const open = (button: HTMLElement) => {
        const rect = button.getBoundingClientRect()
        setMenuAt({x: rect.left, y: rect.top - 6, width: rect.width})
    }

    const icon = (Icon: typeof Settings2) => <Icon width={16} height={16}/>
    const items: ContextMenuItem[] = [
        {label: 'Settings', icon: icon(Settings2), onSelect: () => navigate('/settings')},
        {label: 'Usage', icon: icon(Analytics), onSelect: () => navigate('/stats')},
        {label: 'Models', icon: icon(Cpu), onSelect: () => navigate('/models')},
        ...(user?.role === 'owner' ? [
            {label: 'Plugins', icon: icon(Plug), onSelect: () => navigate('/plugins')},
            {label: 'Users', icon: icon(User), onSelect: () => navigate('/users')},
        ] : []),
        {label: 'Log out', icon: icon(Logout), danger: true, onSelect: () => setConfirmingLogout(true)},
    ]

    return (
        <>
            {/* While the menu is open, a press on the button mustn't count as a click outside it: the menu would close
                and the click open it again */}
            <Div className="account-menu__anchor" onMouseDown={(e) => menuAt && e.stopPropagation()}>
            <Button variant="secondary" className={`account-menu${compact ? ' account-menu--compact' : ''}`}
                    title={user ? `${user.username} (${user.role})` : 'Account'}
                    onClicked={(e) => (menuAt ? setMenuAt(null) : open(e.currentTarget))}>
                {compact ? <User width={18} height={18}/> : (
                    <>
                        <span className="account-menu__name">{user?.username ?? ''}</span>
                        <span className="account-menu__role">{user?.role ?? ''}</span>
                        <ChevronUp width={14} height={14}/>
                    </>
                )}
            </Button>
            </Div>
            {/* Above the button, at least as wide as it: a list that grows out of it */}
            <ContextMenu position={menuAt} corner="bottom-left" minWidth={compact ? undefined : menuAt?.width}
                         onClose={() => setMenuAt(null)} items={items}/>
            <ConfirmPopup open={confirmingLogout} title="Log out" confirmLabel="Log out"
                          message="Log out of this browser? Your chats stay as they are." onConfirm={logout}
                          onClose={() => setConfirmingLogout(false)}/>
        </>
    )
};
