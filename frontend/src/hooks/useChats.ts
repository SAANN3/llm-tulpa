import { useEffect, useState } from 'react'

import { createChat as createChatApi } from '../api/chats/create'
import { deleteChat as deleteChatApi } from '../api/chats/delete'
import { getChats } from '../api/chats/get'
import { renameChat as renameChatApi } from '../api/chats/rename'
import type { ChatOut } from '../api/chats/types'

const CHATS_PAGE_SIZE = 30

/**
 * A chat list, newest-active first: the first page loaded on mount, `loadOlder` to page
 * further back, and `createChat` to add one — appended locally rather than refetched,
 * bumping `total` to keep `loadOlder`'s guard accurate. `onLoaded`, if given, fires once
 * the initial page has landed — not on `loadOlder` — for a caller that wants to snap its
 * scroll view to the top right as the newest chats show up (e.g. the sidebar).
 */
export function useChats(onLoaded?: () => void) {
  const [chats, setChats] = useState<ChatOut[]>([])
  const [total, setTotal] = useState(0)
  const [loadingMore, setLoadingMore] = useState(false)

  useEffect(() => {
    getChats({ limit: CHATS_PAGE_SIZE }).then((result) => {
      setChats('chats' in result ? result.chats : [result])
      setTotal('chats' in result ? result.total : 1)
      onLoaded?.()
    })
  }, [])

  const loadOlder = async () => {
    if (loadingMore || chats.length >= total) return

    setLoadingMore(true)
    try {
      const skip = chats.length
      const result = await getChats({ skip, limit: CHATS_PAGE_SIZE })
      const older = 'chats' in result ? result.chats : [result]
      setChats((prev) => [...prev, ...older])
      setTotal('chats' in result ? result.total : total)
    } finally {
      setLoadingMore(false)
    }
  }

  const createChat = async (name: string) => {
    const created = await createChatApi(name)
    setChats((prev) => [...prev, created])
    setTotal((t) => t + 1)
    return created
  }

  const rename = async (id: number, name: string) => {
    await renameChatApi(id, name)
    setChats((prev) => prev.map((c) => (c.id === id ? { ...c, name } : c)))
  }

  const remove = async (id: number) => {
    await deleteChatApi(id)
    setChats((prev) => prev.filter((c) => c.id !== id))
    setTotal((t) => Math.max(0, t - 1))
  }

  return { chats, loadOlder, createChat, rename, delete: remove }
}
