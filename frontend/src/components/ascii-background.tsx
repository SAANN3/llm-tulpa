import {useEffect, useRef} from 'react'
import '../styles/ascii-background.scss'
import {findBackground, type BackgroundEffect} from '../backgrounds'
import {measurePage} from '../backgrounds/engine/page-layout.ts'
import {BackgroundRunner} from '../backgrounds/engine/runner.ts'
import {useTheme} from '../context/use-theme.ts'

interface CanvasProps {
    effect: BackgroundEffect
    strength: number
    speed: number
}

/** The canvas and its runner, for as long as an animated background is chosen */
const BackgroundCanvas = ({effect, strength, speed}: CanvasProps) => {
    const canvasRef = useRef<HTMLCanvasElement>(null)
    const runnerRef = useRef<BackgroundRunner | null>(null)

    useEffect(() => {
        const canvas = canvasRef.current
        if (!canvas) return
        const runner = new BackgroundRunner(canvas)
        runnerRef.current = runner
        const root = document.documentElement

        runner.setColors(getComputedStyle(root))
        runner.setLayout(measurePage())
        runner.resize(window.innerWidth, window.innerHeight)

        const onResize = () => {
            runner.resize(window.innerWidth, window.innerHeight)
            runner.setLayout(measurePage())
        }
        window.addEventListener('resize', onResize)

        // The theme's colors change with `data-theme` on <html>, which the theme provider sets after this renders
        const themeWatch = new MutationObserver(() => runner.setColors(getComputedStyle(root)))
        themeWatch.observe(root, {attributes: true, attributeFilter: ['data-theme']})

        // The page in front changes with the route, the sidebar folding, the composer growing: read it again after
        // any change to the page, at most once a frame (an unchanged layout costs the effect nothing)
        let pending = 0
        const pageWatch = new MutationObserver(() => {
            if (pending) return
            pending = requestAnimationFrame(() => {
                pending = 0
                runner.setLayout(measurePage())
            })
        })
        pageWatch.observe(document.body, {childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'style']})

        return () => {
            window.removeEventListener('resize', onResize)
            themeWatch.disconnect()
            pageWatch.disconnect()
            cancelAnimationFrame(pending)
            runner.dispose()
            runnerRef.current = null
        }
    }, [])

    useEffect(() => {
        void runnerRef.current?.setEffect(effect)
    }, [effect])

    useEffect(() => {
        runnerRef.current?.setTuning(strength, speed)
    }, [strength, speed])

    return <canvas ref={canvasRef} className="ascii-background" aria-hidden/>
};

/** The animated background behind every page, when one is chosen in Settings (kept per browser) */
export const AsciiBackground = () => {
    const {background} = useTheme()
    const effect = findBackground(background.id)
    return effect ? <BackgroundCanvas effect={effect} strength={background.strength} speed={background.speed}/> : null
};
