import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { ToastContainer } from './toast-container'
import { useToastStore } from '@/stores/toast-store'

// Mutating the zustand store directly (outside an event handler React
// already knows about) needs to be wrapped in act() so React flushes the
// resulting re-render before assertions run.
function show(message: string) {
    act(() => {
        useToastStore.getState().show(message)
    })
}

describe('ToastContainer', () => {
    beforeEach(() => {
        useToastStore.setState({ toasts: [] })
    })

    it('renders nothing when there are no toasts', () => {
        const { container } = render(<ToastContainer />)
        expect(container).toBeEmptyDOMElement()
    })

    it('renders a toast message added to the store', () => {
        render(<ToastContainer />)
        show('Something failed')

        expect(screen.getByText('Something failed')).toBeInTheDocument()
    })

    it('renders multiple queued toasts', () => {
        render(<ToastContainer />)
        show('First')
        show('Second')

        expect(screen.getByText('First')).toBeInTheDocument()
        expect(screen.getByText('Second')).toBeInTheDocument()
    })

    it('dismiss button removes that toast from the store', () => {
        render(<ToastContainer />)
        show('Dismiss me')

        fireEvent.click(screen.getByLabelText('Dismiss'))

        expect(screen.queryByText('Dismiss me')).not.toBeInTheDocument()
        expect(useToastStore.getState().toasts).toHaveLength(0)
    })

    it('dismissing one toast leaves the others', () => {
        render(<ToastContainer />)
        show('Keep')
        show('Remove')

        const dismissButtons = screen.getAllByLabelText('Dismiss')
        fireEvent.click(dismissButtons[1])

        expect(screen.getByText('Keep')).toBeInTheDocument()
        expect(screen.queryByText('Remove')).not.toBeInTheDocument()
    })
})
