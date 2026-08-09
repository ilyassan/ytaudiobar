// Hand-written release highlights shown once via the "What's new" dialog after
// an auto-update lands (never on a fresh install). Add an entry here as part of
// cutting a release with user-facing changes worth calling out -- a version
// with no entry here simply won't show the dialog, it isn't required for every
// release.
export interface WhatsNewEntry {
    version: string
    highlights: string[]
}

export const WHATS_NEW: WhatsNewEntry[] = [
    {
        version: '2.5.0-beta.2',
        highlights: [
            'More reliable downloads — auto-retries when YouTube blocked a request.',
            'Playback recovering when network drops mid-stream.'
        ]
    }
]

export function getWhatsNew(version: string): WhatsNewEntry | undefined {
    return WHATS_NEW.find((entry) => entry.version === version)
}
