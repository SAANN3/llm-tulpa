import axios from 'axios'
import {useEffect, useRef, useState} from 'react'
import {useNavigate, useSearchParams} from 'react-router-dom'
import '../styles/home.scss'
import type {ThinkChoice} from '../api/agent/types'
import {Mark} from '../components/mark.tsx'
import {Div, Label} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {UserInput} from '../components/user-input.tsx'
import {useChats} from '../hooks/use-chats.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {usePrompts} from '../hooks/use-prompts.ts'
import {setPendingPrompt} from '../utils/pending-prompt.ts'

const Home = () => {
    useDocumentTitle('Llm-tulpa')
    const navigate = useNavigate()
    const [searchParams] = useSearchParams()
    const {greet, inputExample, chatName} = usePrompts()
    const {createChat} = useChats()
    const [greeting, setGreeting] = useState('')
    const [greetingLoading, setGreetingLoading] = useState(true)
    const [placeholder, setPlaceholder] = useState('')
    const [creating, setCreating] = useState(false)

    // A prompt handed over by the launcher (extensions/launcher) as /?prompt=..., already percent-decoded
    const launchPrompt = searchParams.get('prompt')?.trim() || null

    // replace swaps this history entry for the chat, so Back can't land on /?prompt=... and send it twice
    const onSend = async (prompt: string, think: ThinkChoice, images: string[], fileIds: number[], replace = false) => {
        setCreating(true)
        try {
            const name = await chatName(prompt, images)
            const chat = await createChat(name)
            setPendingPrompt(chat.id, prompt, think, images, fileIds)
            navigate(`/chat?id=${chat.id}`, {replace})
        } finally {
            setCreating(false)
        }
    }

    // Sent once on mount; the ref rather than the effect's dependencies is what makes it once,
    // since dev-mode double mounting would otherwise send it twice
    const onSendRef = useRef(onSend)
    onSendRef.current = onSend
    const launchedRef = useRef(false)
    useEffect(() => {
        if (!launchPrompt || launchedRef.current) return
        launchedRef.current = true
        void onSendRef.current(launchPrompt, true, [], [], true)
    }, [launchPrompt])

    // Aborted on unmount so orphaned slow requests don't hold connection slots against the backend
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

    const loading = greetingLoading || creating

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="home">
                <Div className="center vbox home__stage">
                    <Mark spinning={loading}/>
                    {loading ? (
                        <Label className="status-line home__status" variant="secondary"
                               text={creating ? 'Starting a chat' : 'Thinking'}/>
                    ) : null}
                    {!greetingLoading && greeting ? (
                        <Label className="greeting home__greeting" text={greeting}/>
                    ) : null}
                    <UserInput
                        className="home__composer"
                        blocked={creating}
                        onSended={onSend}
                        placeholder={placeholder || undefined}
                        clearOnSend={false}
                    />
                </Div>
            </Div>
        </Div>
    )
};

export default Home
