import { Music, X } from 'lucide-react'

interface AppHeaderProps {
    query: string
    onQueryChange: (query: string) => void
    isMusicMode: boolean
    onMusicModeToggle: () => void
}

export function AppHeader({ query, onQueryChange, isMusicMode, onMusicModeToggle }: AppHeaderProps) {
    return (
        <div className="flex-shrink-0 bg-background">
            {/* App Title Section */}
            <div className="px-4 pt-4 pb-3 flex items-center gap-2">
                <Music className="w-5 h-5 text-foreground" />
                <h1 className="text-[15px] font-semibold text-foreground">YTAudioBar</h1>
            </div>

            {/* Search Bar Section */}
            <div className="px-4 pb-3">
                <div className="relative">
                    <input
                        type="text"
                        value={query}
                        onChange={(e) => onQueryChange(e.target.value)}
                        placeholder={isMusicMode ? 'Search YouTube Music...' : 'Search YouTube...'}
                        className="w-full px-3 py-2 pr-24 bg-secondary border-none rounded-lg text-[14px] text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-[var(--macos-blue)]"
                    />

                    {/* Right side buttons */}
                    <div className="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-1">
                        {query && (
                            <button
                                onClick={() => onQueryChange('')}
                                className="w-5 h-5 flex items-center justify-center rounded-full hover-macos-button"
                            >
                                <X className="w-3.5 h-3.5 text-muted-foreground" />
                            </button>
                        )}

                        {/* Music Mode Toggle */}
                        <button
                            onClick={onMusicModeToggle}
                            className={`w-6 h-6 flex items-center justify-center rounded transition-colors ${
                                isMusicMode
                                    ? 'text-[var(--macos-blue)]'
                                    : 'text-muted-foreground hover:text-foreground'
                            }`}
                            title={isMusicMode ? 'YouTube Music' : 'YouTube'}
                        >
                            <Music className="w-4 h-4" />
                        </button>
                    </div>
                </div>
            </div>
        </div>
    )
}
