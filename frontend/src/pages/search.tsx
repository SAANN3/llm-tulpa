import {useNavigate, useSearchParams} from 'react-router-dom'

import '../styles/search.scss'
import {ChatFinder, type FoundTarget} from '../components/chat-finder.tsx'
import {Div} from '../components/primitives'
import {Sidebar} from '../components/sidebar.tsx'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'

/** Search across all of the user's chats — the destination of the sidebar's "Search chats" entry.
 * The query lives in the address (`/search?q=…`), so Back from an opened chat returns to the
 * same results. */
const Search = () => {
    useDocumentTitle('Search chats')
    const navigate = useNavigate()
    const [params, setParams] = useSearchParams()
    const query = params.get('q') ?? ''

    // Replace, so typing doesn't leave one history entry per keystroke
    const setQuery = (value: string) => setParams(value ? {q: value} : {}, {replace: true})

    // A found message opens its chat already scrolled to it: the chat page reads the target from
    // the navigation state (see `ChatView`)
    const openFound = ({chatId, jump}: FoundTarget) =>
        navigate(`/chat?id=${chatId}`, {state: jump ? {jump} : undefined})

    return (
        <Div className="page">
            <Sidebar/>
            <Div className="vbox search-page">
                <TypewriterLabel className="mono search-page__title" text="~/search" charIntervalMs={30}/>
                <ChatFinder query={query} onQueryChanged={setQuery} onSelect={openFound}/>
            </Div>
        </Div>
    )
};

export default Search
