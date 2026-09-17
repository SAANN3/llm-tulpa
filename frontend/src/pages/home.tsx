import axios from 'axios'
import {useEffect, useState} from 'react'
import {useNavigate} from 'react-router-dom'
import '../styles/home.scss'
import type {ThinkChoice} from '../api/agent/types'
import {Div, Label} from '../components/primitives'
import {Mark} from '../components/mark.tsx'
import {Sidebar} from '../components/sidebar.tsx'
import {UserInput} from '../components/user-input.tsx'
import {useChats} from '../hooks/use-chats.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {usePrompts} from '../hooks/use-prompts.ts'
import {setPendingPrompt} from '../utils/pending-prompt.ts'

const Home = () => {
    useDocumentTitle('Llm-tulpa')
    const navigate = useNavigate()
    const {greet, inputExample, chatName} = usePrompts()
    const {createChat} = useChats()
    const [greeting, setGreeting] = useState('')
    const [greetingLoading, setGreetingLoading] = useState(true)
    const [placeholder, setPlaceholder] = useState('')
    const [creating, setCreating] = useState(false)

    useEffect(() => {
        const controller = new AbortController()

        greet(controller.signal)
            .then((text) => {
                setGreeting(text)
                setGreetingLoading(false)
            })
            .catch((err) => {
                if (!axios.isCancel(err)) throw err
            })

        return () => controller.abort()
    }, [greet])

    useEffect(() => {
        const controller = new AbortController()

        inputExample(controller.signal)
            .then(setPlaceholder)
            .catch((err) => {
                if (!axios.isCancel(err)) throw err
            })

        return () => controller.abort()
    }, [inputExample])

    const onSend = async (prompt: string, think: ThinkChoice, images: string[], fileIds: number[]) => {
        setCreating(true)
        try {
            const name = await chatName(prompt, images)
            const chat = await createChat(name)
            setPendingPrompt(chat.id, prompt, think, images, fileIds)
            navigate(`/chat?id=${chat.id}`)
        } finally {
            setCreating(false)
        }
    }

    const loading = greetingLoading || creating

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="home">
                <Div className="center vbox home__stage">
                    <Mark spinning={loading}/>
                    {loading ? (
                        <Label
                            className="status-line home__status"
                            variant="secondary"
                            text={creating ? 'Starting a chat' : 'Thinking'}
                        />
                    ) : null}
                    {!greetingLoading && greeting ? (
                        <Label className="greeting home__greeting" text={greeting}/>
                    ) : null}
                    <UserInput
                        blocked={creating}
                        onSended={onSend}
                        placeholder={placeholder || undefined}
                        clearOnSend={false}
                        className="home__composer"
                    />
                </Div>
            </Div>
        </Div>
    )
};

export default Home
