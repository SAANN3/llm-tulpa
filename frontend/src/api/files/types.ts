export interface FileOut {
    id: number
    /** `null` for a file uploaded before its chat exists (the home composer); claimed when it's attached to a sent message. */
    chat_id: number | null
    full_path: string
    file_name: string
    read_only: boolean
}
