import ReactMarkdown from 'react-markdown'
import remarkBreaks from 'remark-breaks'
import remarkGfm from 'remark-gfm'
import '../styles/chat-message.scss'
import {CodeBlock} from './code-block.tsx'
import {Div} from './primitives'
import {useTyped} from '../hooks/use-typed.ts'

export interface LiveReplyProps {
    thinking: string
    text: string
}

/**
 * Takes a `<think>` block out of text that carries one: a model whose server doesn't separate its reasoning writes it
 * into the reply itself. The backend splits the stored reply the same way, so the live one looks like what it becomes.
 * An open block with no end yet is all thinking; an end with no start means everything before it was.
 */
const splitThink = (thinking: string, text: string): [string, string] => {
    const open = text.indexOf('<think>')
    const close = text.indexOf('</think>')
    if (open < 0 && close < 0) return [thinking, text]
    const start = open < 0 ? 0 : open + '<think>'.length
    const before = open < 0 ? '' : text.slice(0, open)
    if (close < 0) return [thinking + text.slice(start), before]
    return [thinking + text.slice(start, close), before + text.slice(close + '</think>'.length)]
}

const MARKDOWN_COMPONENTS = {pre: CodeBlock}

/**
 * A reply as the model is writing it, shaped like the message it becomes: its thinking while it thinks, then the text,
 * typed out at an even pace (`useTyped`) with a blinking block where the next words land. The thinking shows its newest
 * lines only; the stored message keeps all of it.
 */
export const LiveReplyMessage = ({thinking, text}: LiveReplyProps) => {
    const [fullThought, fullReply] = splitThink(thinking, text)
    const thought = useTyped(fullThought)
    const reply = useTyped(fullReply)
    const writing = reply.trim() !== ''
    return (
        <Div className="chat-message chat-message--assistant live-reply">
            <Div className="vbox chat-message__body">
                {writing ? (
                    <div className="markdown live-reply__text">
                        <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks]} components={MARKDOWN_COMPONENTS}>
                            {reply}
                        </ReactMarkdown>
                    </div>
                ) : (
                    <Div className="chat-message__thinking-body live-reply__thinking">
                        <div>
                            {thought.trimStart()}
                            <span className="live-reply__cursor"/>
                        </div>
                    </Div>
                )}
            </Div>
        </Div>
    )
};
