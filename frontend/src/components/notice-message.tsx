import '../styles/notice-message.scss'
import {Div, Label} from './primitives'

export interface NoticeMessageProps {
    content: string
}

/** A backend-written marker in the timeline (a background job ended), centered and muted like a date separator */
export const NoticeMessage = ({content}: NoticeMessageProps) => (
    <Div className="notice-message">
        <Div className="notice-message__box">
            <Label className="notice-message__text" text={content}/>
        </Div>
    </Div>
);
