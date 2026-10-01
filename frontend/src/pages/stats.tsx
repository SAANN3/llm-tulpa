import {useNavigate, useParams} from 'react-router-dom'
import '../styles/stats.scss'
import {Button, Div} from '../components/primitives'
import {TypewriterLabel} from '../components/typewriter-label.tsx'
import {useDocumentTitle} from '../hooks/use-document-title.ts'
import {ActivityTab} from './stats/activity-tab.tsx'
import {ModelsTab} from './stats/models-tab.tsx'
import {ServerTab} from './stats/server-tab.tsx'
import {SpeedTab} from './stats/speed-tab.tsx'
import {ToolsTab} from './stats/tools-tab.tsx'
import {DEFAULT_RANGE, RANGES, type RangeOption} from './stats/ranges.ts'
import {UsageTab} from './stats/usage-tab.tsx'

const TABS = ['Usage', 'Speed', 'Models', 'Tools', 'Activity', 'Server'] as const
type Tab = (typeof TABS)[number]

/** Usage and performance of the user's chats, in tabs. Every tab but Server covers a range of days;
 * Server is the live state of the model backend. The tab and the range live in the address
 * (`/stats/speed/30d`), so a refresh or a shared link lands on the same view; an address that names
 * neither shows the defaults. */
const Stats = () => {
    useDocumentTitle('Usage')
    const navigate = useNavigate()
    const {tab: tabSlug, range: rangeSlug} = useParams()
    const tab = TABS.find((name) => name.toLowerCase() === tabSlug) ?? TABS[0]
    const range = RANGES.find((option) => option.slug === rangeSlug) ?? DEFAULT_RANGE

    // Replaces the address instead of adding to the history, so Back leaves the page rather than
    // stepping through every tab that was clicked.
    const show = (nextTab: Tab, nextRange: RangeOption) =>
        navigate(`/stats/${nextTab.toLowerCase()}/${nextRange.slug}`, {replace: true})

    return (
        <Div className="page center vbox stats">
            <TypewriterLabel className="stats__title" text="[ Usage ]" charIntervalMs={30}/>
            <Div className="dos-frame stats__panel">
                <span className="dos-frame__title">{tab}</span>
                <Div className="dos-frame__body stats__body">
                    <Div className="stats__bar">
                        <Div className="stats__tabs">
                            {TABS.map((name) => (
                                <Button key={name} variant={name === tab ? 'primary' : 'secondary'} text={name}
                                        onClicked={() => show(name, range)}/>
                            ))}
                        </Div>
                        {tab !== 'Server' ? (
                            <Div className="stats__ranges">
                                {RANGES.map((option) => (
                                    <Button key={option.label} variant={option === range ? 'primary' : 'secondary'}
                                            text={option.label} onClicked={() => show(tab, option)}/>
                                ))}
                            </Div>
                        ) : null}
                    </Div>

                    {tab === 'Usage' ? <UsageTab range={range}/> : null}
                    {tab === 'Speed' ? <SpeedTab range={range}/> : null}
                    {tab === 'Models' ? <ModelsTab range={range}/> : null}
                    {tab === 'Tools' ? <ToolsTab range={range}/> : null}
                    {tab === 'Activity' ? <ActivityTab range={range}/> : null}
                    {tab === 'Server' ? <ServerTab/> : null}

                    <Button variant="secondary" text="Back" onClicked={() => navigate('/')}/>
                </Div>
            </Div>
        </Div>
    )
};

export default Stats
