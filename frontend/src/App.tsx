import { BrowserRouter, Navigate, Outlet, Route, Routes } from 'react-router-dom'

import { SettingsProvider } from './context/SettingsProvider'
import { ThemeProvider } from './context/ThemeProvider'
import { useSettings } from './context/useSettings'
import Chat from './pages/Chat'
import Home from './pages/Home'
import Settings from './pages/Settings'
import Setup from './pages/Setup'

/** Layout route guard for everything except `/setup` — redirects there if settings haven't been configured yet, so pages themselves don't each need this check. Waits out the initial settings fetch first to avoid redirecting before it's known whether settings actually exist. */
function RequireSettings() {
  const { settings, loading } = useSettings()

  if (loading) return null
  if (!settings) return <Navigate to="/setup" replace />

  return <Outlet />
}

function App() {
  return (
    <ThemeProvider>
      <SettingsProvider>
        <BrowserRouter>
          <Routes>
            <Route path="/setup" element={<Setup />} />
            <Route element={<RequireSettings />}>
              <Route path="/" element={<Home />} />
              <Route path="/chat" element={<Chat />} />
              <Route path="/settings" element={<Settings />} />
            </Route>
          </Routes>
        </BrowserRouter>
      </SettingsProvider>
    </ThemeProvider>
  )
}

export default App
