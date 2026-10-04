import axios from 'axios'
import {useEffect, useRef, useState} from 'react'
import {useNavigate, useSearchParams} from 'react-router-dom'
import '../styles/home.scss'
import type {ThinkChoice} from '../api/agent/types'
import type {LaunchProfile} from '../api/profiles/types'
import {Mark} from '../components/mark.tsx'
import {ChooseModelPopup} from '../components/popups/choose-model-popup.tsx'
import {Button, Div, Label} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {UserInput} from '../components/user-input.tsx'
import {useSettings} from '../context/use-settings.ts'
import {useChats} from '../hooks/use-chats.ts'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {usePrompts} from '../hooks/use-prompts.ts'
import {useRuntime} from '../hooks/use-runtime.ts'
import {setPendingPrompt} from '../utils/pending-prompt.ts'

/** A model picked for the chat about to be started, instead of the user's default */
interface StartOn {
    provider: string
    model: string
    /** The launch profile, for a model the backend runs itself; null for an Ollama model */
    profileId: number | null
}

const Home = () => {
    useDocumentTitle('Llm-tulpa')
    const navigate = useNavigate()
    const [searchParams] = useSearchParams()
    const {greet, inputExample, chatName} = usePrompts()
    const {createChat} = useChats()
    const {settings} = useSettings()
    const [greeting, setGreeting] = useState('')
    const [greetingLoading, setGreetingLoading] = useState(true)
    const [placeholder, setPlaceholder] = useState('')
    const [creating, setCreating] = useState(false)
    // For this chat only: the default model in the settings is not touched
    const [startOn, setStartOn] = useState<StartOn | null>(null)
    const [picking, setPicking] = useState(false)

    const pickProfile = (profile: LaunchProfile) => {
        setStartOn({provider: profile.provider, model: profile.model, profileId: profile.id})
        setPicking(false)
    }

    // A prompt handed over by the launcher (extensions/launcher) as /?prompt=..., already percent-decoded
    const launchPrompt = searchParams.get('prompt')?.trim() || null

    // replace swaps this history entry for the chat, so Back can't land on /?prompt=... and send it twice
    const onSend = async (prompt: string, think: ThinkChoice, images: string[], fileIds: number[], replace = false) => {
        setCreating(true)
        try {
            const name = await chatName(prompt, images)
            const chat = await createChat(name, startOn
                ? (startOn.profileId != null ? {launchProfileId: startOn.profileId} : {model: startOn.model, provider: startOn.provider})
                : undefined)
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
                if (axios.isCancel(err)) return
                // No model to answer (the server is down, nothing is set up yet): the page works without a
                // greeting, so it stops waiting for one instead of showing "Thinking" for ever
                setGreetingLoading(false)
            })

        return () => controller.abort()
    }, [greet])

    useEffect(() => {
        const controller = new AbortController()

        inputExample(controller.signal)
            .then(setPlaceholder)
            .catch(() => {
                // Only a suggestion for the composer's placeholder: the default one stays
            })

        return () => controller.abort()
    }, [inputExample])

    const loading = greetingLoading || creating
    // While something here waits on the model server, what it is doing: a model loading is a long wait
    const {status} = useRuntime(loading)
    const statusText = status?.queued
        ? 'Waiting for the model'
        : status?.state === 'starting'
            ? `Loading ${status.model ?? 'the model'}`
            : creating ? 'Starting a chat' : 'Thinking'
    const shownModel = startOn?.model ?? settings?.active_model ?? null
    const shownProvider = startOn?.provider ?? settings?.llm_provider ?? 'llama-cpp'

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="home">
                <Div className="center vbox home__stage">
                    <Mark spinning={loading}/>
                    {loading ? (
                        <Label className="status-line home__status" variant="secondary" text={statusText}/>
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
                        startModel={startOn ? {model: startOn.model, provider: startOn.provider} : null}
                        footerStart={(
                            <Div className="home__model">
                                <Button variant="secondary" className="home__model-button" disabled={creating}
                                        text={`Model: ${shownModel ?? 'none'}${startOn ? ' (this chat)' : ''}`}
                                        onClicked={() => setPicking(true)}/>
                                {startOn ? <Button variant="secondary" className="home__model-reset" text="Default" disabled={creating} onClicked={() => setStartOn(null)}/> : null}
                            </Div>
                        )}
                    />
                    <ChooseModelPopup
                        open={picking}
                        provider={shownProvider}
                        selected={shownProvider === 'ollama' ? shownModel : null}
                        selectedProfileId={startOn ? startOn.profileId : settings?.launch_profile_id ?? null}
                        onSelect={(model) => {
                            setStartOn({provider: 'ollama', model, profileId: null})
                            setPicking(false)
                        }}
                        onSelectProfile={pickProfile}
                        onClose={() => setPicking(false)}
                    />
                </Div>
            </Div>
        </Div>
    )
};

export default Home
