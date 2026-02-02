import { useState, useEffect } from 'react'
import { Shuffle, Repeat, Repeat1, ListMusic, GripVertical } from 'lucide-react'
import { getQueue, getQueueInfo, toggleShuffle, cycleRepeatMode, reorderQueue, type YTVideoInfo, type RepeatMode } from '@/lib/tauri'
import { TrackItem } from '@/components/track-item'
import { TabHeader } from '@/components/tab-header'

export function QueueTab() {
    const [queue, setQueue] = useState<YTVideoInfo[]>([])
    const [queueInfo, setQueueInfo] = useState('')
    const [shuffleMode, setShuffleMode] = useState(false)
    const [repeatMode, setRepeatMode] = useState<RepeatMode>('Off')
    const [draggedIndex, setDraggedIndex] = useState<number | null>(null)

    const loadQueue = async () => {
        try {
            const [queueData, info] = await Promise.all([
                getQueue(),
                getQueueInfo()
            ])
            setQueue(queueData)
            setQueueInfo(info)
        } catch (error) {
            console.error('Failed to load queue:', error)
        }
    }

    useEffect(() => {
        loadQueue()

        // Set up interval to refresh queue
        const interval = setInterval(loadQueue, 2000)
        return () => clearInterval(interval)
    }, [])

    const handleToggleShuffle = async () => {
        try {
            const enabled = await toggleShuffle()
            setShuffleMode(enabled)
            await loadQueue()
        } catch (error) {
            console.error('Failed to toggle shuffle:', error)
        }
    }

    const handleCycleRepeat = async () => {
        try {
            const mode = await cycleRepeatMode()
            setRepeatMode(mode)
        } catch (error) {
            console.error('Failed to cycle repeat mode:', error)
        }
    }

    // Drag and drop handlers
    const handleDragStart = (index: number) => {
        setDraggedIndex(index)
    }

    const handleDragOver = (e: React.DragEvent, index: number) => {
        e.preventDefault()
        if (draggedIndex === null || draggedIndex === index) return

        const newQueue = [...queue]
        const draggedItem = newQueue[draggedIndex]

        // Remove from old position
        newQueue.splice(draggedIndex, 1)

        // Insert at new position
        newQueue.splice(index, 0, draggedItem)

        setQueue(newQueue)
        setDraggedIndex(index)
    }

    const handleDragEnd = async () => {
        setDraggedIndex(null)
        // Update queue order in backend
        try {
            await reorderQueue(queue)
        } catch (error) {
            console.error('Failed to reorder queue:', error)
            // Reload queue on error
            await loadQueue()
        }
    }

    return (
        <div className="flex flex-col h-full bg-background">
            <TabHeader
                title="Queue"
                subtitle={queue.length > 0 ? queueInfo : undefined}
                actions={
                    <>
                        <button
                            onClick={handleToggleShuffle}
                            className={`w-8 h-8 flex items-center justify-center rounded-full hover-macos-button transition-colors ${
                                shuffleMode
                                    ? 'text-[var(--macos-blue)]'
                                    : 'text-muted-foreground'
                            }`}
                            title={shuffleMode ? 'Shuffle On' : 'Shuffle Off'}
                        >
                            <Shuffle className="w-5 h-5" />
                        </button>
                        <button
                            onClick={handleCycleRepeat}
                            className={`w-8 h-8 flex items-center justify-center rounded-full hover-macos-button transition-colors ${
                                repeatMode !== 'Off'
                                    ? 'text-[var(--macos-blue)]'
                                    : 'text-muted-foreground'
                            }`}
                            title={`Repeat ${repeatMode}`}
                        >
                            {repeatMode === 'One' ? (
                                <Repeat1 className="w-5 h-5" />
                            ) : (
                                <Repeat className="w-5 h-5" />
                            )}
                        </button>
                    </>
                }
            />

            {/* Queue Content */}
            <div className="flex-1 overflow-y-auto">
                {queue.length === 0 ? (
                    <div className="flex flex-col items-center justify-center h-full text-center px-6">
                        <ListMusic className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                        <h3 className="text-[15px] font-semibold text-foreground mb-2">
                            Queue is Empty
                        </h3>
                        <p className="text-[13px] text-muted-foreground max-w-[250px]">
                            Use "Play All" on a playlist to add tracks to your queue
                        </p>
                    </div>
                ) : (
                    <div className="py-2">
                        {queue.map((track, index) => (
                            <div
                                key={`${track.id}-${index}`}
                                draggable
                                onDragStart={() => handleDragStart(index)}
                                onDragOver={(e) => handleDragOver(e, index)}
                                onDragEnd={handleDragEnd}
                                className={`group flex items-center gap-1 transition-opacity ${
                                    draggedIndex === index ? 'opacity-50' : ''
                                }`}
                            >
                                {/* Drag Handle */}
                                <div className="pl-1 cursor-grab active:cursor-grabbing">
                                    <GripVertical className="w-4 h-4 text-muted-foreground" />
                                </div>

                                <div className="flex-1 min-w-0 overflow-hidden">
                                    <TrackItem
                                        track={track}
                                        context="queue"
                                        queueIndex={index}
                                    />
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </div>
    )
}
