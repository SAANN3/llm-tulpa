import {useCallback} from 'react'
import {chatName as chatNameApi} from '../api/prompts/chat-name.ts'
import {greet as greetApi} from '../api/prompts/greet'
import {inputExample as inputExampleApi} from '../api/prompts/input-examples.ts'

/** One-shot generations against a predefined prompt, no chat history */
export const usePrompts = () => {
    const greet = useCallback(async (signal?: AbortSignal): Promise<string> => {
        const result = await greetApi(signal)
        return result.response
    }, [])

    const inputExample = useCallback(async (signal?: AbortSignal): Promise<string> => {
        const result = await inputExampleApi(signal)
        return result.text
    }, [])

    const chatName = useCallback(async (content: string, images: string[] = []): Promise<string> => {
        const result = await chatNameApi(content, images)
        return result.response
    }, [])

    return {greet, inputExample, chatName}
};
