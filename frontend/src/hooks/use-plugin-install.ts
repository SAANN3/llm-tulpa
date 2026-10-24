import {useEffect, useRef, useState} from 'react'
import {installStagedPlugin} from '../api/plugins/external-install.ts'
import {getInstallTask} from '../api/plugins/external-install-task.ts'
import {discardStagedPlugin} from '../api/plugins/external-stage-discard.ts'
import {stageExternalPluginGit} from '../api/plugins/external-stage-git.ts'
import {stageExternalPluginFile} from '../api/plugins/external-stage-upload.ts'
import type {InstallTask, StagedPlugin} from '../api/plugins/types.ts'
import {errorReason} from '../utils/error-reason.ts'

const POLL_MS = 1000

/** Installing a plugin in the two steps the backend has: it is staged (unpacked and described, nothing of it runs) and, once the
 * owner has looked, installed as a background task that is followed here. `onInstalled` runs when an install ends well. */
export const usePluginInstall = (onInstalled: () => void) => {
    const [staged, setStaged] = useState<StagedPlugin | null>(null)
    const [task, setTask] = useState<InstallTask | null>(null)
    const [error, setError] = useState<string | null>(null)
    const [busy, setBusy] = useState(false)
    const installed = useRef(onInstalled)
    installed.current = onInstalled

    const running = task?.state === 'running'
    const taskId = task?.id
    useEffect(() => {
        if (!running || !taskId) return
        const poll = setInterval(async () => {
            try {
                const latest = await getInstallTask(taskId)
                setTask(latest)
                if (latest.state === 'done') installed.current()
            } catch {
                // The next tick asks again
            }
        }, POLL_MS)
        return () => clearInterval(poll)
    }, [running, taskId])

    const stage = async (request: () => Promise<StagedPlugin>) => {
        setBusy(true)
        setError(null)
        try {
            setStaged(await request())
            setTask(null)
        } catch (e) {
            setError(errorReason(e, 'Could not read that plugin.'))
        } finally {
            setBusy(false)
        }
    }

    return {
        staged,
        task,
        error,
        busy,
        stageFile: (file: File) => stage(() => stageExternalPluginFile(file)),
        stageGit: (url: string, gitRef: string) => stage(() => stageExternalPluginGit(url, gitRef)),
        /** Starts the install of what is staged */
        install: async () => {
            if (!staged) return
            setError(null)
            try {
                setTask(await installStagedPlugin(staged.id))
            } catch (e) {
                setError(errorReason(e, 'Could not start the install.'))
            }
        },
        /** Throws away what is staged (before an install), or just closes the report of one that ended */
        dismiss: async () => {
            const leaving = staged
            const ended = task != null
            setStaged(null)
            setTask(null)
            setError(null)
            // An install that ended has already removed its staging; one that was only looked at is removed now
            if (leaving && !ended) await discardStagedPlugin(leaving.id).catch(() => undefined)
        },
    }
};
