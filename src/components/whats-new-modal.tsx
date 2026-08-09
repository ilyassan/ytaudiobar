import { Sparkles } from 'lucide-react'
import type { WhatsNewEntry } from '@/lib/whats-new'

interface WhatsNewModalProps {
    entry: WhatsNewEntry
    onClose: () => void
}

export function WhatsNewModal({ entry, onClose }: WhatsNewModalProps) {
    return (
        <div
            className="fixed inset-0 bg-black/50 flex items-center justify-center z-[100]"
            onClick={onClose}
        >
            <div
                className="bg-card rounded-xl w-[320px] max-h-[70vh] flex flex-col overflow-hidden"
                onClick={(e) => e.stopPropagation()}
            >
                <div className="flex items-center gap-2 px-4 py-3 border-b border-macos-separator">
                    <Sparkles className="w-4 h-4 text-macos-blue flex-shrink-0" />
                    <h3 className="text-[13px] font-semibold text-foreground">
                        What&apos;s new in {entry.version}
                    </h3>
                </div>

                <ul className="flex-1 overflow-y-auto px-4 py-3 space-y-2">
                    {entry.highlights.map((highlight, i) => (
                        <li
                            key={i}
                            className="text-[13px] text-foreground leading-snug flex gap-2"
                        >
                            <span className="text-macos-blue flex-shrink-0">
                                •
                            </span>
                            <span>{highlight}</span>
                        </li>
                    ))}
                </ul>

                <div className="px-4 py-3 border-t border-macos-separator">
                    <button
                        onClick={onClose}
                        className="w-full py-2 rounded-lg bg-macos-blue text-white text-[13px] font-medium hover:opacity-90 transition-opacity"
                    >
                        Got it
                    </button>
                </div>
            </div>
        </div>
    )
}
