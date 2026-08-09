import { useState, useEffect, useRef } from 'react'
import { isMac } from '@/utils/platform'
import { Folder, Github, AlertCircle, RefreshCw, Mail } from 'lucide-react'
import { open } from '@tauri-apps/plugin-shell'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import {
    getDownloadsDirectory,
    setDownloadsDirectory,
    getAudioQuality,
    setAudioQuality as saveAudioQuality,
    getAppVersion,
    checkForUpdatesManual,
    getAutostartEnabled,
    setAutostartEnabled as saveAutostartEnabled
} from '@/lib/tauri'
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue
} from '@/components/ui/select'
import { useToastStore } from '@/stores/toast-store'

const AUDIO_QUALITY_OPTIONS = [
    { value: 'best', label: 'Best Available' },
    { value: '320', label: '320 kbps' },
    { value: '256', label: '256 kbps' },
    { value: '192', label: '192 kbps' },
    { value: '128', label: '128 kbps' }
]

export function SettingsTab() {
    const [downloadLocation, setDownloadLocation] = useState('')
    const [audioQuality, setAudioQuality] = useState('best')
    const [appVersion, setAppVersion] = useState('1.0.0')
    const [autostartEnabled, setAutostartEnabled] = useState(false)
    const [isLoading, setIsLoading] = useState(true)
    const [isMigrating, setIsMigrating] = useState(false)
    const [isCheckingUpdates, setIsCheckingUpdates] = useState(false)
    const [updateMessage, setUpdateMessage] = useState('')
    const updateMessageTimeoutRef = useRef<ReturnType<
        typeof setTimeout
    > | null>(null)

    useEffect(() => {
        return () => {
            if (updateMessageTimeoutRef.current) {
                clearTimeout(updateMessageTimeoutRef.current)
            }
        }
    }, [])

    // Load settings from backend
    useEffect(() => {
        const loadSettings = async () => {
            try {
                const [location, quality, version, autostart] =
                    await Promise.all([
                        getDownloadsDirectory(),
                        getAudioQuality(),
                        getAppVersion(),
                        getAutostartEnabled()
                    ])
                setDownloadLocation(location)
                setAudioQuality(quality)
                setAppVersion(version)
                setAutostartEnabled(autostart)
            } catch (error) {
                console.error('Failed to load settings:', error)
                useToastStore.getState().show('Failed to load settings')
            } finally {
                setIsLoading(false)
            }
        }
        loadSettings()
    }, [])

    const handleAutostartToggle = async () => {
        const newValue = !autostartEnabled
        setAutostartEnabled(newValue)
        try {
            await saveAutostartEnabled(newValue)
        } catch (error) {
            console.error('Failed to set autostart:', error)
            useToastStore.getState().show('Failed to change autostart setting')
            setAutostartEnabled(!newValue)
        }
    }

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
                    console.error('Failed to change download location:', error)
                    useToastStore
                        .getState()
                        .show(
                            typeof error === 'string'
                                ? error
                                : 'Failed to change download location'
                        )
                } finally {
                    setIsMigrating(false)
                }
            }
        } catch (error) {
            console.error('Failed to open folder picker:', error)
            useToastStore.getState().show('Failed to open folder picker')
        }
    }

    const handleQualityChange = async (quality: string) => {
        setAudioQuality(quality)
        try {
            await saveAudioQuality(quality)
        } catch (error) {
            console.error('Failed to save audio quality:', error)
            useToastStore.getState().show('Failed to save audio quality')
        }
    }

    const handleOpenGitHub = () => {
        open('https://github.com/ilyassan/ytaudiobar')
    }

    const handleReportIssue = () => {
        open('https://github.com/ilyassan/ytaudiobar/issues/new')
    }

    const handleContactUs = () => {
        open('mailto:contact@ytaudiobar.com')
    }

    // Clear message after 5 seconds, replacing any dismissal still pending from
    // an earlier check so it can't wipe the message this one just set.
    const scheduleUpdateMessageDismiss = () => {
        if (updateMessageTimeoutRef.current) {
            clearTimeout(updateMessageTimeoutRef.current)
        }
        updateMessageTimeoutRef.current = setTimeout(
            () => setUpdateMessage(''),
            5000
        )
    }

    const handleCheckUpdates = async () => {
        setIsCheckingUpdates(true)
        setUpdateMessage('Checking for updates...')
        try {
            await checkForUpdatesManual()
            setUpdateMessage(
                'Update check complete! Check console logs for details.'
            )
            scheduleUpdateMessageDismiss()
        } catch (error) {
            console.error('Failed to check for updates:', error)
            setUpdateMessage(
                'Failed to check for updates. See console for details.'
            )
            scheduleUpdateMessageDismiss()
        } finally {
            setIsCheckingUpdates(false)
        }
    }

    if (isLoading) {
        return (
            <div className="flex items-center justify-center h-full">
                <div className="text-[13px] text-muted-foreground">
                    Loading settings...
                </div>
            </div>
        )
    }

    return (
        <div className="flex flex-col h-full overflow-y-auto bg-background">
            <div className="p-5">
                {/* General Section */}
                <section className="mb-8">
                    <h2 className="text-[20px] font-semibold text-foreground mb-4">
                        General
                    </h2>

                    {/* Download Location — fixed on macOS (~/Music/YTAudioBar Downloads),
                        user-configurable on Windows/Linux */}
                    {!isMac && (
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
                                        isMigrating
                                            ? 'opacity-50 cursor-not-allowed'
                                            : ''
                                    }`}
                                >
                                    <Folder className="w-4 h-4" />
                                    {isMigrating ? 'Moving...' : 'Change'}
                                </button>
                            </div>
                            <p className="text-[11px] text-muted-foreground mt-1">
                                Downloaded audio files will be saved to this
                                folder
                            </p>
                        </div>
                    )}

                    {/* Audio Quality */}
                    <div className="mb-4">
                        <label className="block text-[13px] font-medium text-foreground mb-2">
                            Audio Quality
                        </label>
                        <Select
                            value={audioQuality}
                            onValueChange={handleQualityChange}
                        >
                            <SelectTrigger className="w-full bg-secondary hover:bg-secondary/80 border-none text-[13px]">
                                <SelectValue placeholder="Select quality" />
                            </SelectTrigger>
                            <SelectContent className="bg-card border-macos-separator">
                                {AUDIO_QUALITY_OPTIONS.map((option) => (
                                    <SelectItem
                                        key={option.value}
                                        value={option.value}
                                        className="text-[13px] focus:bg-secondary cursor-pointer"
                                    >
                                        {option.label}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                        <p className="text-[11px] text-muted-foreground mt-1">
                            Higher quality means larger file sizes
                        </p>
                    </div>

                    {/* Launch at Startup */}
                    <div className="flex items-center justify-between py-2">
                        <div>
                            <div className="text-[13px] font-medium text-foreground">
                                Launch at startup
                            </div>
                            <div className="text-[11px] text-muted-foreground">
                                Automatically start YTAudioBar when your PC
                                starts
                            </div>
                        </div>
                        <button
                            onClick={handleAutostartToggle}
                            className={`relative w-11 h-6 rounded-full transition-colors duration-200 flex-shrink-0 ${
                                autostartEnabled
                                    ? 'bg-[var(--macos-blue)]'
                                    : 'bg-white/20'
                            }`}
                            aria-label="Toggle launch at startup"
                        >
                            <span
                                className={`absolute top-0.5 left-0.5 w-5 h-5 bg-white rounded-full shadow transition-transform duration-200 ${
                                    autostartEnabled
                                        ? 'translate-x-5'
                                        : 'translate-x-0'
                                }`}
                            />
                        </button>
                    </div>
                </section>

                {/* Divider */}
                <div className="h-[1px] bg-muted-foreground/20 mb-8" />

                {/* About Section */}
                <section>
                    <h2 className="text-[20px] font-semibold text-foreground mb-4">
                        About
                    </h2>

                    {/* App Version */}
                    <div className="mb-4">
                        <div className="text-[13px] text-foreground font-medium">
                            YTAudioBar
                        </div>
                        <div className="text-[11px] text-muted-foreground">
                            Version {appVersion}
                        </div>
                    </div>

                    {/* Check for Updates Button */}
                    <div className="mb-4">
                        <button
                            onClick={handleCheckUpdates}
                            disabled={isCheckingUpdates}
                            className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg bg-[var(--macos-blue)] text-white hover:opacity-90 transition-opacity disabled:opacity-50"
                        >
                            <RefreshCw
                                className={`w-4 h-4 ${isCheckingUpdates ? 'animate-spin' : ''}`}
                            />
                            <span className="text-[13px] font-medium">
                                {isCheckingUpdates
                                    ? 'Checking...'
                                    : 'Check for Updates'}
                            </span>
                        </button>
                        {updateMessage && (
                            <p className="text-[11px] text-muted-foreground mt-2 text-center">
                                {updateMessage}
                            </p>
                        )}
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
                        <button
                            onClick={handleContactUs}
                            className="w-full flex items-center gap-3 px-3 py-2 hover-macos-button rounded-lg text-[13px] text-foreground transition-colors"
                        >
                            <Mail className="w-5 h-5" />
                            <span>Contact Us</span>
                        </button>
                    </div>
                </section>
            </div>
        </div>
    )
}
