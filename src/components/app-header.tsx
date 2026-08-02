import { X, Music, ListMusic, Minus, Move, Shrink, Expand } from 'lucide-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isMac } from '@/utils/platform'

interface AppHeaderProps {
    query: string
    onQueryChange: (query: string) => void
    isPlaylistMode: boolean
    isShrinked: boolean
    onPlaylistModeToggle: () => void
    onIsShrinkedToggle: () => void
    onResetWindow: () => void
}

export function AppHeader({
    query,
    onQueryChange,
    isPlaylistMode,
    isShrinked,
    onPlaylistModeToggle,
    onIsShrinkedToggle,
    onResetWindow
}: AppHeaderProps) {
    return (
        <div className="flex-shrink-0 bg-background">
            {/* App Title Section — draggable on Windows/Linux, fixed on macOS */}
            <div
                className={`px-4 pt-4 pb-3 flex items-center gap-2 select-none ${!isMac ? 'cursor-grab active:cursor-grabbing' : ''}`}
                onMouseDown={(e) => {
                    if (!isMac && e.button === 0)
                        getCurrentWindow().startDragging()
                }}
            >
                <img src="/icon.png" alt="YTAudioBar" className="w-5 h-5" />
                <h1 className="text-[15px] font-semibold text-foreground">
                    YTAudioBar
                </h1>
                <div
                    className="ml-auto flex items-center gap-1"
                    onMouseDown={(e) => e.stopPropagation()}
                >
                    <button
                        onClick={onIsShrinkedToggle}
                        className="w-6 h-6 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors"
                        title={isShrinked ? 'Expand' : 'Shrink'}
                    >
                        {isShrinked ? (
                            <Expand className="w-4 h-4" />
                        ) : (
                            <Shrink className="w-4 h-4" />
                        )}
                    </button>
                    <button
                        onClick={onResetWindow}
                        className="w-6 h-6 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors"
                        title="Reset position & size"
                    >
                        <Move className="w-4 h-4" />
                    </button>
                    {/* Minimize only makes sense on Windows/Linux */}
                    {!isMac && (
                        <button
                            onClick={() => getCurrentWindow().minimize()}
                            className="w-6 h-6 flex items-center justify-center rounded hover:bg-secondary text-muted-foreground hover:text-foreground transition-colors"
                            title="Minimize"
                        >
                            <Minus className="w-4 h-4" />
                        </button>
                    )}
                </div>
            </div>

            {!isShrinked && (
                /* Search Bar Section */
                <div className="px-4 pb-3">
                    <div className="relative">
                        <input
                            type="text"
                            value={query}
                            onChange={(e) => onQueryChange(e.target.value)}
                            placeholder={
                                isPlaylistMode
                                    ? 'Search YouTube Playlists...'
                                    : 'Search YouTube...'
                            }
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

                            {/* Search Mode Switch: Tracks vs Playlists */}
                            <div className="relative flex items-center bg-background/60 rounded-full p-0.5 gap-0.5">
                                {/* Sliding active-segment indicator */}
                                <div
                                    className={`absolute top-0.5 left-0.5 w-6 h-5 rounded-full bg-[var(--macos-blue)] transition-transform duration-200 ease-out ${
                                        isPlaylistMode
                                            ? 'translate-x-[26px]'
                                            : 'translate-x-0'
                                    }`}
                                />
                                <button
                                    onClick={() =>
                                        isPlaylistMode && onPlaylistModeToggle()
                                    }
                                    className={`relative z-10 w-6 h-5 flex items-center justify-center rounded-full transition-colors ${
                                        !isPlaylistMode
                                            ? 'text-white'
                                            : 'text-muted-foreground hover:text-foreground'
                                    }`}
                                    title="Search tracks"
                                >
                                    <Music className="w-3 h-3" />
                                </button>
                                <button
                                    onClick={() =>
                                        !isPlaylistMode &&
                                        onPlaylistModeToggle()
                                    }
                                    className={`relative z-10 w-6 h-5 flex items-center justify-center rounded-full transition-colors ${
                                        isPlaylistMode
                                            ? 'text-white'
                                            : 'text-muted-foreground hover:text-foreground'
                                    }`}
                                    title="Search playlists"
                                >
                                    <ListMusic className="w-3 h-3" />
                                </button>
                            </div>
                        </div>
                    </div>
                </div>
            )}
        </div>
    )
}
