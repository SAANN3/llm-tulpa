import {BrowserRouter, Navigate, Outlet, Route, Routes} from 'react-router-dom'
import {AuthProvider} from './context/auth-provider.tsx'
import {SettingsProvider} from './context/settings-provider.tsx'
import {SetupProvider} from './context/setup-provider.tsx'
import {ThemeProvider} from './context/theme-provider.tsx'
import {useAuth} from './context/use-auth.ts'
import {useSetup} from './context/use-setup.ts'
import Chat from './pages/chat.tsx'
import Home from './pages/home.tsx'
import Login from './pages/login.tsx'
import Plugins from './pages/plugins.tsx'
import Settings from './pages/settings.tsx'
import Setup from './pages/setup/setup.tsx'
import Users from './pages/users.tsx'

const RequireAuth = () => {
    const {status, loading: setupLoading} = useSetup()
    const {token, loading: authLoading} = useAuth()

    if (setupLoading || authLoading || !status) return null
    if (!status.configured || !status.has_owner) return <Navigate to="/setup" replace/>
    if (!token) return <Navigate to="/login" replace/>
    return <Outlet/>
};

const RequireOwner = () => {
    const {user} = useAuth()
    return user?.role === 'owner' ? <Outlet/> : <Navigate to="/" replace/>
};

const App = () => (
    <ThemeProvider>
        <SetupProvider>
            <AuthProvider>
                <SettingsProvider>
                    <BrowserRouter>
                        <Routes>
                            <Route path="/setup" element={<Setup/>}/>
                            <Route path="/login" element={<Login/>}/>
                            <Route element={<RequireAuth/>}>
                                <Route path="/" element={<Home/>}/>
                                <Route path="/chat" element={<Chat/>}/>
                                <Route path="/settings" element={<Settings/>}/>
                                <Route element={<RequireOwner/>}>
                                    <Route path="/plugins" element={<Plugins/>}/>
                                    <Route path="/users" element={<Users/>}/>
                                </Route>
                            </Route>
                        </Routes>
                    </BrowserRouter>
                </SettingsProvider>
            </AuthProvider>
        </SetupProvider>
    </ThemeProvider>
);

export default App
