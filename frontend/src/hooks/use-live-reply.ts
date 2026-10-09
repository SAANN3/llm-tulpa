import {useEffect, useState} from 'react'
import type {ReplySoFar} from '../api/agent/types.ts'
import {useServerEvent} from './use-server-events.ts'

/** What the model has written so far of one reply */
export interface LiveReply {
    /** Which model call of the run wrote it, and how many of its pieces are in `thinking` and `text` */
    number: number
    seq: number
    thinking: string
    text: string
    /** Its model call's reply was stored: it stays shown until the page has read the stored message */
    stored: boolean
    /** Whether `thinking` and `text` hold the reply from its start; one this page joined midway (or that missed a
     * piece) isn't shown until a state read fills it in */
    complete: boolean
    /** Pieces that came while the reply wasn't complete, for adding after the state read's text */
    early: Piece[]
}

interface Piece {
    seq: number
    thinking: string
    text: string
}

/** Adds the pieces that follow on from `reply`, in order; one out of turn means one was missed */
const extend = (reply: LiveReply, pieces: Piece[]): LiveReply => {
    let next = reply
    for (const piece of [...pieces].sort((a, b) => a.seq - b.seq)) {
        if (piece.seq <= next.seq) continue
        if (piece.seq !== next.seq + 1) return {...next, complete: false, early: pieces.filter((p) => p.seq > next.seq)}
        next = {...next, seq: piece.seq, thinking: next.thinking + piece.thinking, text: next.text + piece.text}
    }
    return {...next, early: []}
}

const isShown = (reply: LiveReply) => reply.complete && (reply.thinking !== '' || reply.text !== '')

/**
 * The replies of a chat's run as the model writes them, from the `reply_piece` events. A run asks the model once per
 * step, so a reply ends where the backend stores something (`messages_changed`); it is kept, marked stored, until
 * `settle` is called once the page shows the stored messages, so the live text and the stored one never both show
 * and never neither. A page that opens in the middle of a reply gets what was written so far from the turn's state
 * (`seed`) and goes on from there with the pieces numbered after it. With `enabled` off the pieces are ignored and
 * nothing is shown.
 */
export const useLiveReply = (chatId: number, enabled: boolean) => {
    const [replies, setReplies] = useState<LiveReply[]>([])

    useEffect(() => {
        setReplies([])
    }, [chatId, enabled])

    useServerEvent('reply_piece', (event) => {
        if (!enabled || event.chat_id !== chatId) return
        const piece = {seq: event.seq, thinking: event.thinking, text: event.text}
        setReplies((now) => {
            const last = now[now.length - 1]
            if (last != null && !last.stored && last.number === event.reply) {
                return [...now.slice(0, -1), last.complete ? extend(last, [piece]) : {...last, early: [...last.early, piece]}]
            }
            // A new model call; one still open before it was dropped (asked for again, or abandoned)
            const kept = last != null && !last.stored ? now.slice(0, -1) : now
            const fresh: LiveReply = {number: event.reply, seq: 0, thinking: '', text: '', stored: false, complete: true, early: []}
            return [...kept, extend(fresh, [piece])]
        })
    })

    useServerEvent('reply_restart', (event) => {
        if (event.chat_id !== chatId) return
        setReplies((now) => now.filter((reply) => reply.stored))
    })

    useServerEvent('messages_changed', (event) => {
        if (event.chat_id !== chatId) return
        setReplies((now) => now.filter(isShown).map((reply) => ({...reply, stored: true})))
    })

    // A stopped or failed run stored nothing of the reply it was writing
    useServerEvent('run_ended', (event) => {
        if (event.chat_id !== chatId) return
        setReplies((now) => now.filter((reply) => reply.stored))
    })

    /** What the turn's state says was written so far: the start of a reply this page joined midway */
    const seed = (so: ReplySoFar) => {
        if (!enabled) return
        setReplies((now) => {
            const last = now[now.length - 1]
            const open = last != null && !last.stored ? last : null
            // Already further than the state (it was read before pieces this page has), or a reply that is over
            if (open != null && (open.number > so.number || (open.number === so.number && open.complete && open.seq >= so.seq))) return now
            if (now.some((reply) => reply.stored && reply.number >= so.number)) return now
            const base: LiveReply = {number: so.number, seq: so.seq, thinking: so.thinking, text: so.text, stored: false, complete: true, early: []}
            const early = open != null && open.number === so.number ? open.early : []
            return [...now.filter((reply) => reply.stored), extend(base, early)]
        })
    }

    /** The page now shows what was stored: the stored replies' live copies go */
    const settle = () => setReplies((now) => (now.some((reply) => reply.stored) ? now.filter((reply) => !reply.stored) : now))

    const last = replies[replies.length - 1]
    /** A reply is being written that this page can't show yet: it joined midway or missed a piece */
    const missing = last != null && !last.stored && !last.complete

    return {replies: replies.filter(isShown), missing, seed, settle}
}
