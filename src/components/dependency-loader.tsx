import { useState, useEffect } from 'react'

interface DependencyLoaderProps {
    status: 'checking' | 'downloading-ytdlp' | 'downloading-ffmpeg' | 'complete'
    progress: number // 0-100 real percentage
    error: 'connection' | 'unknown' | null
}

export function DependencyLoader({
    status,
    progress,
    error
}: DependencyLoaderProps) {
    const [retryCountdown, setRetryCountdown] = useState(5)

    // Count down from 5 whenever an error is shown
    useEffect(() => {
        if (!error) {
            setRetryCountdown(5)
            return
        }
        setRetryCountdown(5)
        const interval = setInterval(() => {
            setRetryCountdown((n) => {
                if (n <= 1) {
                    clearInterval(interval)
                    return 0
                }
                return n - 1
            })
        }, 1000)
        return () => clearInterval(interval)
    }, [error])

    const getStatusMessage = () => {
        switch (status) {
            case 'checking':
                return 'Checking dependencies...'
            case 'downloading-ytdlp':
                return 'Downloading yt-dlp...'
            case 'downloading-ffmpeg':
                return 'Downloading ffmpeg...'
            case 'complete':
                return 'Ready!'
            default:
                return 'Initializing...'
        }
    }

    return (
        <div className="flex h-screen items-center justify-center bg-background select-none rounded-[12px] overflow-hidden border border-white/10">
            <div className="w-72 space-y-5">
                {/* App Icon + Name */}
                <div className="flex items-center gap-2">
                    <img src="/icon.png" alt="YTAudioBar" className="w-5 h-5" />
                    <span className="text-[15px] font-semibold text-foreground">
                        YTAudioBar
                    </span>
                </div>

                {error ? (
                    /* Error state — retry is automatic, never lets user past this screen */
                    <div className="space-y-3">
                        <p className="text-[13px] text-macos-red font-medium">
                            {error === 'connection'
                                ? 'No internet connection'
                                : 'Download failed'}
                        </p>
                        <p className="text-[12px] text-muted-foreground leading-relaxed">
                            {error === 'connection'
                                ? 'YTAudioBar needs to download yt-dlp and ffmpeg to work. Please check your connection.'
                                : 'Could not download required tools. This may be a temporary issue.'}
                        </p>
                        <p className="text-[11px] text-muted-foreground">
                            Retrying in {retryCountdown}s...
                        </p>
                    </div>
                ) : (
                    /* Normal download progress */
                    <>
                        <p className="text-[13px] text-muted-foreground">
                            {getStatusMessage()}
                        </p>
                        <div className="space-y-1.5">
                            <div className="h-1.5 w-full bg-secondary rounded-full overflow-hidden">
                                <div
                                    className="h-full bg-[var(--macos-blue)] rounded-full transition-all duration-300 ease-out"
                                    style={{
                                        width: `${Math.round(progress)}%`
                                    }}
                                />
                            </div>
                            <p className="text-[11px] text-muted-foreground text-right">
                                {Math.round(progress)}%
                            </p>
                        </div>
                    </>
                )}
            </div>
        </div>
    )
}
