/** Asks the browser for notification permission, must be called from a user gesture */
export const requestNotificationPermission = async (): Promise<boolean> => {
    if (!('Notification' in window)) return false
    if (Notification.permission === 'granted') return true
    if (Notification.permission === 'denied') return false

    const result = await Notification.requestPermission()
    return result === 'granted'
};

/** How long a tab waits for the others' claims on a notification before deciding whether it is the one to show it */
const CLAIM_WAIT_MS = 150

interface Claim {
    tag: string
    tab: string
    /** Whether the tab that sent it is on screen right now */
    visible: boolean
}

const TAB_ID = Math.random().toString(36).slice(2)

/** How long what the other tabs said about a notification is kept: tabs hear the same event at slightly different times */
const CLAIM_KEEP_MS = 10_000

/** Tabs of the app talk over one channel, so that a notification is shown once however many are open */
const channel = typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel('llm-tulpa-notifications')
const claims = new Map<string, Claim[]>()

/** Kept from the first word on a tag, not only once this tab has the event itself: another tab can hear it first */
const remember = (claim: Claim) => {
    const known = claims.get(claim.tag)
    if (known) {
        known.push(claim)
        return
    }
    claims.set(claim.tag, [claim])
    setTimeout(() => claims.delete(claim.tag), CLAIM_KEEP_MS)
}

channel?.addEventListener('message', (event: MessageEvent<Claim>) => remember(event.data))

/**
 * Whether this tab should show the notification `tag`: no tab of the app is on screen (the user is looking at
 * one, so a pop-up would only repeat it), and of the hidden tabs this one has the lowest id. Every tab runs the same
 * check on the same event, so exactly one wins; a tab that is on screen still says so, which is how the hidden ones
 * know to stay quiet. Without a channel (an old browser) every hidden tab shows its own.
 */
const isMyTurn = async (tag: string): Promise<boolean> => {
    const visible = !document.hidden
    if (!channel) return !visible

    channel.postMessage({tag, tab: TAB_ID, visible} satisfies Claim)
    if (visible) return false

    await new Promise((resolve) => setTimeout(resolve, CLAIM_WAIT_MS))
    const others = claims.get(tag) ?? []

    return !others.some((claim) => claim.tab !== TAB_ID && (claim.visible || claim.tab < TAB_ID))
};

/**
 * Shows a browser notification if permission was granted, no tab of the app is on screen, and no other tab is
 * showing it already. `tag` names the event the notification is about (the same one in every tab, e.g. a chat's
 * run): it is how tabs recognise they are talking about the same thing, and the browser itself replaces a
 * notification that has the tag of one it already shows.
 */
export const notify = async (title: string, text: string, tag: string): Promise<void> => {
    if (!('Notification' in window)) return
    if (Notification.permission !== 'granted') return
    if (!(await isMyTurn(tag))) return

    new Notification(title, {body: text, tag})
};
