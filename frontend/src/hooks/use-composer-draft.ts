import {useEffect, useRef, useState} from 'react'
import {clearDraft, getDraft, setDraft} from '../utils/draft-storage.ts'

/** What the composer shows while a message is being edited. `key` tells one edit from the next. */
export interface ComposerEdit {
    key: number
    text: string
    images: string[]
    fileIds: number[]
}

/**
 * The composer's text and attachments, kept per chat in sessionStorage.
 * `draft-storage` is only the read and write. This hook decides when: load on the key,
 * save as the user types, put the draft back when an edit is cancelled, and drop it once a send lands.
 */
export const useComposerDraft = (draftKey: string | undefined, editing: ComposerEdit | null | undefined, initialText?: string) => {
    const [initialDraft] = useState(() => (draftKey != null ? getDraft(draftKey) : null))
    const [value, setValue] = useState(initialText ?? initialDraft?.text ?? '')
    const [images, setImages] = useState<string[]>(initialDraft?.images ?? [])
    const [fileIds, setFileIds] = useState<number[]>(initialDraft?.fileIds ?? [])
    // The key can change under a live composer, so the persist effect reads it from a ref
    const draftKeyRef = useRef(draftKey)
    draftKeyRef.current = draftKey
    // Set by cancelEdit. The effect then puts the draft back and skips one persist, so the
    // edited message's text (still in state) isn't saved over that draft. A send does not set
    // this: the box keeps whatever is in it, including a prompt whose send failed after the old message was removed.
    const restoreDraftRef = useRef(false)
    const skipPersistRef = useRef(false)

    const editKey = editing?.key
    const wasEditingRef = useRef(false)
    useEffect(() => {
        if (editing) {
            setValue(editing.text)
            setImages(editing.images)
            setFileIds(editing.fileIds)
        } else if (wasEditingRef.current && restoreDraftRef.current) {
            restoreDraftRef.current = false
            const draft = draftKeyRef.current != null ? getDraft(draftKeyRef.current) : null
            setValue(draft?.text ?? '')
            setImages(draft?.images ?? [])
            setFileIds(draft?.fileIds ?? [])
            skipPersistRef.current = true
        }
        wasEditingRef.current = editing != null
        // eslint-disable-next-line react-hooks/exhaustive-deps -- reacts to a new edit, not to the object's identity
    }, [editKey])

    // A new chat under the same composer: load that chat's draft. The first paint already
    // used it, so this matters when the key changes. Re-saving the same draft is harmless.
    useEffect(() => {
        if (draftKey == null) return
        const draft = getDraft(draftKey)
        setValue(draft?.text ?? '')
        setImages(draft?.images ?? [])
        setFileIds(draft?.fileIds ?? [])
    }, [draftKey])

    // Skipped while a message is being edited (the box holds that message, not a draft),
    // and for one tick after a cancel (see skipPersistRef)
    useEffect(() => {
        if (editing) return
        if (skipPersistRef.current) {
            skipPersistRef.current = false
            return
        }
        const key = draftKeyRef.current
        if (key == null) return
        if (value === '' && images.length === 0 && fileIds.length === 0) clearDraft(key)
        else setDraft(key, {text: value, images, fileIds})
    }, [value, images, fileIds, editing])

    const cancelEdit = () => {
        restoreDraftRef.current = true
    }

    /** The send landed. Drop the draft, and the field too unless the page is about to leave it behind. */
    const sent = (clearFields: boolean) => {
        if (clearFields) {
            setValue('')
            setImages([])
            setFileIds([])
        }
        const key = draftKeyRef.current
        if (key != null) clearDraft(key)
    }

    return {value, setValue, images, setImages, fileIds, setFileIds, cancelEdit, sent}
}
