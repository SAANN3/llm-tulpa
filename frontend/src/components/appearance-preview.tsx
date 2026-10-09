import {useEffect, useLayoutEffect, useRef, useState} from 'react'
import '../styles/appearance-preview.scss'
import {findBackground, type BackgroundId} from '../backgrounds'
import {BackgroundRunner} from '../backgrounds/engine/runner.ts'
import type {PageLayout} from '../backgrounds/engine/types.ts'
import type {ThemeName} from '../themes'
import {formatTime, type TimeFormat} from '../utils/time-format.ts'
import {Div, Label} from './primitives'

/** The sidebar's width on a real page (`.sidebar`) */
const SIDEBAR_PX = 240

export interface AppearancePreviewProps {
    theme: ThemeName
    background: BackgroundId
    strength: number
    speed: number
    sidebarSeeThrough: boolean
    timeFormat: TimeFormat
    /** The reply is typed out over and over, as a live reply shows */
    streamReplies: boolean
    /** What is shown, in the corner */
    note: string
}

/**
 * A chat page in miniature, with a theme and a background that need not be the ones in use: the page and the real
 * background effect are laid out at the window's own size and shrunk to fit, so it shows what the page would look
 * like, whole, with no preview of its own to write for any background. The theme is set on the miniature alone
 * (`data-theme` on it), so the page around it keeps its colors.
 */
const REPLY = 'Like this: the page in front, the background behind it.'

/** How much of `text` is typed so far: a few letters at a time, then a rest with all of it shown, then again */
const useTyping = (text: string, on: boolean): number => {
    const [typed, setTyped] = useState(text.length)
    useEffect(() => {
        if (!on) {
            setTyped(text.length)
            return
        }
        let shown = 0
        let rest = 0
        const id = setInterval(() => {
            if (shown < text.length) shown = Math.min(text.length, shown + 3)
            else if (++rest > 25) {
                shown = 0
                rest = 0
            }
            setTyped(shown)
        }, 80)
        return () => clearInterval(id)
    }, [text, on])
    return typed
}

export const AppearancePreview = ({theme, background, strength, speed, sidebarSeeThrough, timeFormat, streamReplies, note}: AppearancePreviewProps) => {
    const boxRef = useRef<HTMLDivElement>(null)
    const pageRef = useRef<HTMLDivElement>(null)
    const canvasRef = useRef<HTMLCanvasElement>(null)
    const runnerRef = useRef<BackgroundRunner | null>(null)
    const [size] = useState(() => ({width: window.innerWidth, height: window.innerHeight}))
    const [scale, setScale] = useState(0.4)
    const effect = findBackground(background)
    const typed = useTyping(REPLY, streamReplies)
    const typing = typed < REPLY.length

    // Shrinks the window-sized page to the box it is shown in
    useLayoutEffect(() => {
        const box = boxRef.current
        if (!box) return
        const fit = () => setScale(Math.min(box.clientWidth / size.width, box.clientHeight / size.height))
        fit()
        const watch = new ResizeObserver(fit)
        watch.observe(box)
        return () => watch.disconnect()
    }, [size])

    useEffect(() => {
        const canvas = canvasRef.current
        if (!canvas) return
        const runner = new BackgroundRunner(canvas)
        runnerRef.current = runner
        // A chat page: the sidebar on the left, the composer's rows covered at the bottom
        const layout: PageLayout = {kind: 'chat', sidebarPx: SIDEBAR_PX, contentPx: 880, coveredRows: 11}
        runner.setLayout(layout)
        runner.resize(size.width, size.height)
        if (pageRef.current) runner.setColors(getComputedStyle(pageRef.current))
        return () => {
            runner.dispose()
            runnerRef.current = null
        }
    }, [size])

    // After the miniature has its theme attribute, so its computed colors are the theme's
    useLayoutEffect(() => {
        if (pageRef.current) runnerRef.current?.setColors(getComputedStyle(pageRef.current))
    }, [theme])

    // Only the one effect shown runs; with none (off, or the static dots) nothing is drawn
    useEffect(() => {
        if (effect) void runnerRef.current?.setEffect(effect)
        else runnerRef.current?.stop()
    }, [effect])

    useEffect(() => {
        runnerRef.current?.setTuning(strength, speed)
    }, [strength, speed])

    const at = (h: number, m: number) => formatTime(new Date(2026, 0, 1, h, m, 12), timeFormat)

    return (
        <Div ref={boxRef} className="appearance-preview" style={{aspectRatio: `${size.width} / ${size.height}`}}>
            <div ref={pageRef} data-theme={theme} inert
                 className={`appearance-preview__page${background === 'dots' ? ' appearance-preview__page--dots' : ''}`}
                 style={{width: size.width, height: size.height, transform: `scale(${scale})`}}>
                <canvas ref={canvasRef} className="appearance-preview__canvas" style={{visibility: effect ? 'visible' : 'hidden'}}/>
                <div className={`appearance-preview__sidebar${sidebarSeeThrough ? ' appearance-preview__sidebar--see-through' : ''}`}>
                    <div className="appearance-preview__new-chat">New chat</div>
                    <span className="appearance-preview__heading">Utils</span>
                    <span>Search chats</span>
                    <span>Settings</span>
                    <span>Usage</span>
                    <span className="appearance-preview__heading">Chats</span>
                    <span className="appearance-preview__chat appearance-preview__chat--on">Port the backgrounds</span>
                    <span className="appearance-preview__chat">Cool anime girl</span>
                    <span className="appearance-preview__chat">Fix the login page</span>
                </div>
                <div className="appearance-preview__main">
                    <div className="appearance-preview__title">Port the backgrounds ⌄</div>
                    <div className="appearance-preview__messages">
                        <div className="appearance-preview__bubble">How does this theme look with the background?</div>
                        <span className="appearance-preview__time appearance-preview__time--user">{at(14, 11)}</span>
                        <div className="appearance-preview__reply">
                            {REPLY.slice(0, typed)}
                            {typing ? <span className="caret">_</span> : null}
                        </div>
                        <span className="appearance-preview__time" style={{visibility: typing ? 'hidden' : 'visible'}}>{`${at(14, 12)}, spent 312 tokens`}</span>
                    </div>
                    <div className="appearance-preview__composer">Message…</div>
                </div>
            </div>
            <Label variant="secondary" className="appearance-preview__note" text={note}/>
        </Div>
    )
};
