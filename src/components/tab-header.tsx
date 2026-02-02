import { ReactNode } from 'react'

interface TabHeaderProps {
    title: string
    subtitle?: string
    actions?: ReactNode
}

export function TabHeader({ title, subtitle, actions }: TabHeaderProps) {
    return (
        <div className="flex items-center justify-between px-4 py-3 border-b border-macos-separator bg-card flex-shrink-0">
            <div className="flex-1 min-w-0">
                <h2 className="text-[17px] font-semibold text-foreground">
                    {title}
                </h2>
                {subtitle && (
                    <div className="text-[12px] text-muted-foreground">
                        {subtitle}
                    </div>
                )}
            </div>
            {actions && (
                <div className="flex items-center gap-2 flex-shrink-0">
                    {actions}
                </div>
            )}
        </div>
    )
}
