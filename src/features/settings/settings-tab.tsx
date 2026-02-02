import { useState, useEffect } from 'react'
import { Folder, Github, AlertCircle } from 'lucide-react'
import { open } from '@tauri-apps/plugin-shell'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import {
    getDownloadsDirectory,
    setDownloadsDirectory,
    getAudioQuality,
    setAudioQuality as saveAudioQuality,
    getAppVersion
} from '@/lib/tauri'

const AUDIO_QUALITY_OPTIONS = [
    { value: 'best', label: 'Best Available' },
    { value: '320', label: '320 kbps' },
    { value: '256', label: '256 kbps' },
    { value: '192', label: '192 kbps' },
    { value: '128', label: '128 kbps' },
]

export function SettingsTab() {
    const [downloadLocation, setDownloadLocation] = useState('')
    const [audioQuality, setAudioQuality] = useState('best')
    const [appVersion, setAppVersion] = useState('1.0.0')
    const [isLoading, setIsLoading] = useState(true)
    const [isMigrating, setIsMigrating] = useState(false)

    // Load settings from backend
    useEffect(() => {
        const loadSettings = async () => {
            try {
                const [location, quality, version] = await Promise.all([
                    getDownloadsDirectory(),
                    getAudioQuality(),
                    getAppVersion()
                ])
                setDownloadLocation(location)
                setAudioQuality(quality)
                setAppVersion(version)
            } catch (error) {
                console.error('Failed to load settings:', error)
            } finally {
                setIsLoading(false)
            }
        }
        loadSettings()
    }, [])

    const handleChangeDownloadLocation = async () => {
        try {
            const selected = await openDialog({
                directory: true,
                multiple: false,
                title: 'Select Download Location'
            })

            if (selected && typeof selected === 'string') {
                setIsMigrating(true)
                try {
                    await setDownloadsDirectory(selected)
                    setDownloadLocation(selected)
                } catch (error: any) {
                    // Show error to user
                    alert(error || 'Failed to change download location')
                    console.error('Failed to change download location:', error)
                } finally {
                    setIsMigrating(false)
                }
            }
        } catch (error) {
            console.error('Failed to open folder picker:', error)
        }
    }

    const handleQualityChange = async (e: React.ChangeEvent<HTMLSelectElement>) => {
        const quality = e.target.value
        setAudioQuality(quality)
        try {
            await saveAudioQuality(quality)
        } catch (error) {
            console.error('Failed to save audio quality:', error)
        }
    }

    const handleOpenGitHub = () => {
        open('https://github.com/ilyassan/ytaudiobar')
    }

    const handleReportIssue = () => {
        open('https://github.com/ilyassan/ytaudiobar/issues/new')
    }

    if (isLoading) {
        return (
            <div className="flex items-center justify-center h-full">
                <div className="text-[13px] text-muted-foreground">Loading settings...</div>
            </div>
        )
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
                                disabled={isMigrating}
                                className={`px-4 py-2 bg-secondary hover-macos-button rounded-lg text-[13px] text-foreground font-medium transition-colors flex items-center gap-2 ${
                                    isMigrating ? 'opacity-50 cursor-not-allowed' : ''
                                }`}
                            >
                                <Folder className="w-4 h-4" />
                                {isMigrating ? 'Moving...' : 'Change'}
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
                        <select
                            value={audioQuality}
                            onChange={handleQualityChange}
                            className="w-full px-3 py-2 bg-secondary rounded-lg text-[13px] text-foreground border-none outline-none focus:ring-2 focus:ring-[var(--macos-blue)] transition-all"
                        >
                            {AUDIO_QUALITY_OPTIONS.map((option) => (
                                <option key={option.value} value={option.value}>
                                    {option.label}
                                </option>
                            ))}
                        </select>
                        <p className="text-[11px] text-muted-foreground mt-1">
                            Higher quality means larger file sizes
                        </p>
                    </div>
                </section>

                {/* Divider */}
                <div className="h-[1px] bg-muted-foreground/20 mb-8" />

                {/* About Section */}
                <section>
                    <h2 className="text-[20px] font-semibold text-foreground mb-4">About</h2>

                    {/* App Version */}
                    <div className="mb-4">
                        <div className="text-[13px] text-foreground font-medium">
                            YTAudioBar
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                            Version {appVersion}
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
