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
        version: '2.5.0',
        highlights: [
            'More reliable downloads — auto-retries when YouTube blocked a request.',
            'Playback recovering when network drops mid-stream, with an accurate position.',
            "New installs now save downloads to your Music folder instead of Downloads, so a Downloads cleanup can't take them with it."
        ]
    },
    {
        version: '2.6.0-beta.1',
        highlights: [
            'You can now open local audio files directly with YTAudioBar (double-click, or "Open with") — title, duration, and cover art are read from the file itself.'
        ]
    },
    {
        version: '2.6.0-beta.2',
        highlights: [
            'Fixed YTAudioBar not showing up in "Open with" for audio files on Windows, Linux, and macOS.'
        ]
    },
    {
        version: '2.6.0-beta.3',
        highlights: [
            'Local playback now recognizes many more audio formats (m4b, wma, ape, aiff, amr, and more), not just the most common ones.'
        ]
    }
]

export function getWhatsNew(version: string): WhatsNewEntry | undefined {
    return WHATS_NEW.find((entry) => entry.version === version)
}
