import axios from 'axios'
import {BACKEND_URL} from '../../config'

/** Fired on `window` after a chat's folder assignment changes, carrying `{chatId, folderId}`
 * in `detail` — lets a folder's own detail page (`/folders/:id`), a separate component
 * instance from whatever UI made the change, know to refetch instead of showing a stale
 * chat that's no longer (or is now newly) in it. */
export const CHAT_FOLDER_CHANGED_EVENT = 'chat-folder-changed'

/** Moves a chat into a folder, or out of one (`folderId: null`) */
export const setChatFolder = async (chatId: number, folderId: number | null): Promise<void> => {
    await axios.post(`${BACKEND_URL}/api/chats/folder`, {chat_id: chatId, folder_id: folderId})
    window.dispatchEvent(new CustomEvent(CHAT_FOLDER_CHANGED_EVENT, {detail: {chatId, folderId}}))
};
