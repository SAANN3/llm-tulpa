import {StrictMode} from 'react'
import {createRoot} from 'react-dom/client'
import App from './app.tsx'
import '@fontsource/jetbrains-mono/latin-400.css'
import '@fontsource/jetbrains-mono/latin-700.css'
import '@fontsource/jetbrains-mono/cyrillic-400.css'
import '@fontsource/jetbrains-mono/cyrillic-700.css'
import './styles/utilities.scss'
import './styles/markdown.scss'

createRoot(document.getElementById('root')!).render(
    <StrictMode>
        <App/>
    </StrictMode>,
)
