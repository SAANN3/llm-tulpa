import {useCallback, useEffect, useRef, useState} from 'react'
import {useServerEvent} from './use-server-events.ts'
import {unseenEndOf} from '../utils/run-end.ts'
import {CHAT_FOLDER_CHANGED_EVENT} from '../api/chats/set-folder'
import {createChat as createChatApi, type ChatStart} from '../api/chats/create'
import {deleteChat as deleteChatApi} from '../api/chats/delete'
import {getChats} from '../api/chats/get'
import {renameChat as renameChatApi} from '../api/chats/rename'
import type {ChatOut, UnseenEnd} from '../api/chats/types'

const CHATS_PAGE_SIZE = 30

/** A chat list, newest-active first, with loadOlder to page back and createChat to add one.
 * `folderId` scopes the list to one folder's chats — omitted, every chat regardless of folder.
 * When scoped, also reloads from the top whenever any chat's folder changes elsewhere (the
 * chat header, or the sidebar's own assign-folder action) — otherwise a chat moved out of
 * this folder from another component would keep showing here until a manual refresh. */
export const useChats = (onLoaded?: () => void, folderId?: number) => {
    const [chats, setChats] = useState<ChatOut[]>([])
    const [total, setTotal] = useState(0)
    const [loadingMore, setLoadingMore] = useState(false)

    const reload = useCallback(
        () =>
            getChats({folder_id: folderId, limit: CHATS_PAGE_SIZE}).then((result) => {
                setChats('chats' in result ? result.chats : [result])
                setTotal('chats' in result ? result.total : 1)
                onLoaded?.()
            }),
        [folderId],
    )

    useEffect(() => {
        reload()
    }, [reload])

    useEffect(() => {
        if (folderId == null) return
        window.addEventListener(CHAT_FOLDER_CHANGED_EVENT, reload)
        return () => window.removeEventListener(CHAT_FOLDER_CHANGED_EVENT, reload)
    }, [folderId, reload])

    const chatsRef = useRef(chats)
    chatsRef.current = chats

    // The user's other pages and devices change the list too
    useServerEvent('chat_created', async (event) => {
        if (chatsRef.current.some((c) => c.id === event.chat_id)) return
        try {
            const created = await getChats({id: event.chat_id})
            // The page that made the chat has it already; a sub-agent's chat is not listed, and a folder's list only has its own
            if ('chats' in created || chatsRef.current.some((c) => c.id === created.id)) return
            if (created.parent_chat_id != null || (folderId != null && created.folder_id !== folderId)) return
            setChats((prev) => [created, ...prev])
            setTotal((t) => t + 1)
        } catch {
            // Deleted again before it could be read: nothing to list
        }
    })
    useServerEvent('chat_renamed', (event) => {
        setChats((prev) => prev.map((c) => (c.id === event.chat_id ? {...c, name: event.name} : c)))
    })
    useServerEvent('chat_deleted', (event) => {
        if (!chatsRef.current.some((c) => c.id === event.chat_id)) return
        setChats((prev) => prev.filter((c) => c.id !== event.chat_id))
        setTotal((t) => Math.max(0, t - 1))
    })

    /** Sets how a chat's last run ended, when the list has the chat */
    const setUnseenEnd = (chatId: number, unseenEnd: UnseenEnd | null) =>
        setChats((prev) => (prev.some((c) => c.id === chatId) ? prev.map((c) => (c.id === chatId ? {...c, unseen_end: unseenEnd} : c)) : prev))

    useServerEvent('run_started', (event) => setUnseenEnd(event.chat_id, null))
    useServerEvent('run_ended', (event) => setUnseenEnd(event.chat_id, unseenEndOf(event.reason)))
    useServerEvent('chat_seen', (event) => setUnseenEnd(event.chat_id, null))

    // Events sent while the stream was down, or while the tab was in the background, are gone: what the newest chats
    // say about their last run is read again
    const syncUnseenEnds = useCallback(async () => {
        try {
            const result = await getChats({folder_id: folderId, limit: CHATS_PAGE_SIZE})
            const fresh = new Map(('chats' in result ? result.chats : [result]).map((c) => [c.id, c.unseen_end]))
            setChats((prev) => prev.map((c) => (fresh.has(c.id) ? {...c, unseen_end: fresh.get(c.id) ?? null} : c)))
        } catch {
            // The next event or sync tries again
        }
    }, [folderId])
    useServerEvent('stream_open', () => void syncUnseenEnds())
    useEffect(() => {
        const onVisible = () => {
            if (document.visibilityState === 'visible') void syncUnseenEnds()
        }
        document.addEventListener('visibilitychange', onVisible)
        return () => document.removeEventListener('visibilitychange', onVisible)
    }, [syncUnseenEnds])

    const loadOlder = async () => {
        if (loadingMore || chats.length >= total) return

        setLoadingMore(true)
        try {
            const skip = chats.length
            const result = await getChats({folder_id: folderId, skip, limit: CHATS_PAGE_SIZE})
            const older = 'chats' in result ? result.chats : [result]
            setChats((prev) => [...prev, ...older])
            setTotal('chats' in result ? result.total : total)
        } finally {
            setLoadingMore(false)
        }
    }

    const createChat = async (name: string, start?: ChatStart) => {
        const created = await createChatApi(name, start)
        setChats((prev) => [created, ...prev])
        setTotal((t) => t + 1)
        return created
    }

    const rename = async (id: number, name: string) => {
        await renameChatApi(id, name)
        setChats((prev) => prev.map((c) => (c.id === id ? {...c, name} : c)))
    }

    const remove = async (id: number) => {
        await deleteChatApi(id)
        setChats((prev) => prev.filter((c) => c.id !== id))
        setTotal((t) => Math.max(0, t - 1))
    }

    return {chats, loadOlder, createChat, rename, delete: remove}
};
