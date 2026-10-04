import {useCallback, useEffect, useState} from 'react'
import {CHAT_FOLDER_CHANGED_EVENT} from '../api/chats/set-folder'
import {createChat as createChatApi, type ChatStart} from '../api/chats/create'
import {deleteChat as deleteChatApi} from '../api/chats/delete'
import {getChats} from '../api/chats/get'
import {renameChat as renameChatApi} from '../api/chats/rename'
import type {ChatOut} from '../api/chats/types'

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
        setChats((prev) => [...prev, created])
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
