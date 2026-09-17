import {BrowserRouter, Navigate, Outlet, Route, Routes} from 'react-router-dom'
import {SettingsProvider} from './context/settings-provider.tsx'
import {ThemeProvider} from './context/theme-provider.tsx'
import {useSettings} from './context/use-settings.ts'
import Chat from './pages/chat.tsx'
import Home from './pages/home.tsx'
import Plugins from './pages/plugins.tsx'
import Settings from './pages/settings.tsx'
import Setup from './pages/setup.tsx'

/** Route guard for everything except /setup, redirecting there until settings are configured */
const RequireSettings = () => {
    const {settings, loading} = useSettings()

    if (loading) return null
    if (!settings) return <Navigate to="/setup" replace/>

    return <Outlet/>
};

const App = () => (
    <ThemeProvider>
        <SettingsProvider>
            <BrowserRouter>
                <Routes>
                    <Route path="/setup" element={<Setup/>}/>
                    <Route element={<RequireSettings/>}>
                        <Route path="/" element={<Home/>}/>
                        <Route path="/chat" element={<Chat/>}/>
                        <Route path="/settings" element={<Settings/>}/>
                        <Route path="/plugins" element={<Plugins/>}/>
                    </Route>
                </Routes>
            </BrowserRouter>
        </SettingsProvider>
    </ThemeProvider>
);

export default App
