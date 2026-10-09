import {useEffect, useState, type ReactNode} from 'react'
import {BrowserRouter, Navigate, Outlet, Route, Routes, useNavigate} from 'react-router-dom'
import {AsciiBackground} from './components/ascii-background.tsx'
import {BackendUnreachable} from './components/backend-unreachable.tsx'
import {SetupUpdatePopup} from './components/popups/setup-update-popup.tsx'
import {AuthProvider} from './context/auth-provider.tsx'
import {DebugPanel} from './components/debug-panel.tsx'
import {RunsProvider} from './context/runs-provider.tsx'
import {SettingsProvider} from './context/settings-provider.tsx'
import {SetupProvider} from './context/setup-provider.tsx'
import {ThemeProvider} from './context/theme-provider.tsx'
import {useAuth} from './context/use-auth.ts'
import {useSetup} from './context/use-setup.ts'
import ChatInfo from './pages/chat-info.tsx'
import Chat from './pages/chat.tsx'
import Folder from './pages/folder.tsx'
import Folders from './pages/folders.tsx'
import Search from './pages/search.tsx'
import Home from './pages/home.tsx'
import Login from './pages/login.tsx'
import Models from './pages/models.tsx'
import Plugins from './pages/plugins.tsx'
import Settings from './pages/settings.tsx'
import SystemPrompt from './pages/settings/system-prompt.tsx'
import Setup from './pages/setup/setup.tsx'
import Stats from './pages/stats.tsx'
import Users from './pages/users.tsx'

const UPDATE_DISMISSED_KEY = 'setup_update_dismissed'

/** How often a page that can't reach the backend asks again */
const BACKEND_RETRY_MS = 4000

/**
 * Covers the app while the backend can't be reached, and asks it again until it answers. The pages stay
 * mounted under the cover, so what the user had open is still there when the backend comes back.
 */
const BackendGate = ({children}: { children: ReactNode }) => {
    const {status, unreachable, refresh} = useSetup()

    useEffect(() => {
        if (unreachable == null) return
        const id = setInterval(() => void refresh(), BACKEND_RETRY_MS)
        return () => clearInterval(id)
    }, [unreachable, refresh])

    return (
        <>
            {/* Before the first answer there is nothing to show under the cover: the guards below would only wait */}
            {status ? children : null}
            {unreachable != null ? <BackendUnreachable reason={unreachable} onRetry={() => void refresh()}/> : null}
        </>
    )
};

const RequireAuth = () => {
    const {status, loading: setupLoading} = useSetup()
    const {token, user, loading: authLoading} = useAuth()
    const navigate = useNavigate()
    const [dismissed, setDismissed] = useState(() => {
        try {
            return sessionStorage.getItem(UPDATE_DISMISSED_KEY) === '1'
        } catch {
            return false
        }
    })

    if (setupLoading || authLoading || !status) return null
    if (!status.configured || !status.has_owner) return <Navigate to="/setup" replace/>
    if (!token) return <Navigate to="/login" replace/>
    return (
        <>
            <Outlet/>
            <SetupUpdatePopup
                open={user?.role === 'owner' && status.update_available && !dismissed}
                onSetup={() => navigate('/setup/update')}
                onLater={() => {
                    setDismissed(true)
                    try {
                        sessionStorage.setItem(UPDATE_DISMISSED_KEY, '1')
                    } catch {
                        // Without storage the prompt simply comes back on the next load.
                    }
                }}
            />
        </>
    )
};

const RequireOwner = () => {
    const {user} = useAuth()
    return user?.role === 'owner' ? <Outlet/> : <Navigate to="/" replace/>
};

const App = () => (
    <ThemeProvider>
        <AsciiBackground/>
        <SetupProvider>
            <BackendGate>
                <AuthProvider>
                    <SettingsProvider>
                        <BrowserRouter>
                            <RunsProvider>
                                <Routes>
                                    <Route path="/setup" element={<Setup/>}/>
                                    <Route path="/setup/update" element={<Setup/>}/>
                                    <Route path="/login" element={<Login/>}/>
                                    <Route element={<RequireAuth/>}>
                                        <Route path="/" element={<Home/>}/>
                                        <Route path="/chat" element={<Chat/>}/>
                                        <Route path="/chat/:id/info/:tab?" element={<ChatInfo/>}/>
                                        <Route path="/search" element={<Search/>}/>
                                        <Route path="/folders" element={<Folders/>}/>
                                        <Route path="/folders/:id" element={<Folder/>}/>
                                        <Route path="/settings" element={<Settings/>}/>
                                        <Route path="/settings/system-prompt" element={<SystemPrompt/>}/>
                                        <Route path="/stats/:tab?/:range?" element={<Stats/>}/>
                                        <Route path="/models/:tab?" element={<Models/>}/>
                                        <Route element={<RequireOwner/>}>
                                            <Route path="/plugins" element={<Plugins/>}/>
                                            <Route path="/users" element={<Users/>}/>
                                        </Route>
                                    </Route>
                                </Routes>
                                <DebugPanel/>
                            </RunsProvider>
                        </BrowserRouter>
                    </SettingsProvider>
                </AuthProvider>
            </BackendGate>
        </SetupProvider>
    </ThemeProvider>
);

export default App
