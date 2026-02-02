import { useState, useEffect } from 'react'
import { Folder, Github, AlertCircle } from 'lucide-react'
import { open } from '@tauri-apps/plugin-shell'

// TODO: Add Tauri commands for settings management
// import { getDownloadLocation, setDownloadLocation, getAudioQuality, setAudioQuality } from '@/lib/tauri'

const AUDIO_QUALITY_OPTIONS = [
    { value: 'best', label: 'Best Available' },
    { value: '320', label: '320 kbps' },
    { value: '256', label: '256 kbps' },
    { value: '192', label: '192 kbps' },
    { value: '128', label: '128 kbps' },
]

export function SettingsTab() {
    const [downloadLocation, setDownloadLocation] = useState('~/Music/YTAudioBar')
    const [audioQuality, setAudioQuality] = useState('best')

    // TODO: Load settings from backend
    useEffect(() => {
        // Load initial settings
        // getDownloadLocation().then(setDownloadLocation)
        // getAudioQuality().then(setAudioQuality)
    }, [])

    const handleChangeDownloadLocation = async () => {
        // TODO: Implement folder picker
        console.log('Change download location')
        // const location = await selectFolder()
        // if (location) {
        //     await setDownloadLocation(location)
        //     setDownloadLocation(location)
        // }
    }

    const handleQualityChange = async (quality: string) => {
        setAudioQuality(quality)
        // TODO: Save to backend
        // await setAudioQuality(quality)
    }

    const handleOpenGitHub = () => {
        open('https://github.com/yourusername/ytaudiobar')
    }

    const handleReportIssue = () => {
        open('https://github.com/yourusername/ytaudiobar/issues/new')
    }

    return (
        <div className="flex flex-col h-full overflow-y-auto bg-background">
            <div className="p-5">
                {/* Downloads Section */}
                <section className="mb-8">
                    <h2 className="text-[20px] font-semibold text-foreground mb-4">Downloads</h2>

                    {/* Download Location */}
                    <div className="mb-4">
                        <label className="block text-[13px] font-medium text-foreground mb-2">
                            Download Location
                        </label>
                        <div className="flex items-center gap-2">
                            <div className="flex-1 px-3 py-2 bg-secondary rounded-lg text-[13px] text-foreground truncate">
                                {downloadLocation}
                            </div>
                            <button
                                onClick={handleChangeDownloadLocation}
                                className="px-4 py-2 bg-secondary hover-macos-button rounded-lg text-[13px] text-foreground font-medium transition-colors flex items-center gap-2"
                            >
                                <Folder className="w-4 h-4" />
                                Change
                            </button>
                        </div>
                        <p className="text-[11px] text-muted-foreground mt-1">
                            Downloaded audio files will be saved to this folder
                        </p>
                    </div>

                    {/* Audio Quality */}
                    <div className="mb-4">
                        <label className="block text-[13px] font-medium text-foreground mb-2">
                            Audio Quality
                        </label>
                        <div className="space-y-1">
                            {AUDIO_QUALITY_OPTIONS.map((option) => (
                                <button
                                    key={option.value}
                                    onClick={() => handleQualityChange(option.value)}
                                    className={`w-full text-left px-3 py-2 rounded-lg text-[13px] transition-colors ${
                                        audioQuality === option.value
                                            ? 'bg-[var(--macos-blue)]/10 text-[var(--macos-blue)] font-medium'
                                            : 'text-foreground hover-macos-button'
                                    }`}
                                >
                                    {option.label}
                                </button>
                            ))}
                        </div>
                    </div>
                </section>

                {/* Divider */}
                <div className="h-[1px] bg-muted-foreground/20 mb-8" />

                {/* About Section */}
                <section>
                    <h2 className="text-[20px] font-semibold text-foreground mb-4">About</h2>

                    {/* App Version */}
                    <div className="mb-4">
                        <div className="text-[13px] text-muted-foreground">
                            YTAudioBar
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                            Version 1.0.0
                        </div>
                    </div>

                    {/* Links */}
                    <div className="space-y-2">
                        <button
                            onClick={handleOpenGitHub}
                            className="w-full flex items-center gap-3 px-3 py-2 hover-macos-button rounded-lg text-[13px] text-foreground transition-colors"
                        >
                            <Github className="w-5 h-5" />
                            <span>View on GitHub</span>
                        </button>
                        <button
                            onClick={handleReportIssue}
                            className="w-full flex items-center gap-3 px-3 py-2 hover-macos-button rounded-lg text-[13px] text-foreground transition-colors"
                        >
                            <AlertCircle className="w-5 h-5" />
                            <span>Report an Issue</span>
                        </button>
                    </div>
                </section>
            </div>
        </div>
    )
}
