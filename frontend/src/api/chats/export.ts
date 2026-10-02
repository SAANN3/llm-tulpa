import axios from 'axios'
import {BACKEND_URL} from '../../config'

export type ExportFormat = 'pdf' | 'md'

/** What becomes of the images and files of the messages: left out, put in the document itself, or saved in a zip next to it */
export type AttachmentMode = 'none' | 'embed' | 'zip'

export interface ExportChatParams {
    chatId: number
    format: ExportFormat
    tools: boolean
    attachments: AttachmentMode
}

/** The file the backend built, and what kind it is: a zip when attachments came along with the document */
export interface ExportedChat {
    blob: Blob
    extension: 'pdf' | 'md' | 'zip'
}

/** Exports a chat as a document, through axios so the Bearer token is attached */
export async function exportChat({chatId, format, tools, attachments}: ExportChatParams): Promise<ExportedChat> {
    try {
        const response = await axios.get<Blob>(`${BACKEND_URL}/api/chats/export`, {
            params: {chat_id: chatId, format, tools, attachments},
            responseType: 'blob',
        })
        const type = String(response.headers['content-type'] ?? '')
        const extension = type.includes('zip') ? 'zip' : type.includes('pdf') ? 'pdf' : 'md'
        return {blob: response.data, extension}
    } catch (e) {
        // With `responseType: 'blob'` an error body arrives as a blob too, so the backend's own
        // explanation has to be read out of it for the caller to show
        let message: string | undefined
        if (axios.isAxiosError(e) && e.response?.data instanceof Blob) {
            try {
                message = (JSON.parse(await e.response.data.text()) as { error?: string }).error
            } catch {
                // Not the backend's JSON error (a proxy's page, say): the generic message stands
            }
        }
        throw new Error(message ?? "Couldn't export that chat.")
    }
}
