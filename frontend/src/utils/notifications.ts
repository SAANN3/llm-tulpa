/** Asks the browser for notification permission, must be called from a user gesture */
export const requestNotificationPermission = async (): Promise<boolean> => {
    if (!('Notification' in window)) return false
    if (Notification.permission === 'granted') return true
    if (Notification.permission === 'denied') return false

    const result = await Notification.requestPermission()
    return result === 'granted'
};

/** Shows a browser notification if permission was granted and the tab isn't focused */
export const notify = (title: string, text: string): void => {
    if (!('Notification' in window)) return
    if (Notification.permission !== 'granted') return
    if (!document.hidden) return

    new Notification(title, {body: text})
};
