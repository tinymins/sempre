import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { Modal } from '@acme/components'
import { AcmeContentBoundary } from './AcmeContentBoundary'

describe('Modal', () => {
  afterEach(() => cleanup())

  it('uses dialog semantics and owns the standard footer and mask behavior', async () => {
    const cancel = vi.fn()
    render(
      <AcmeContentBoundary>
        <Modal open title="Edit subscription" onCancel={cancel}>Modal body</Modal>
      </AcmeContentBoundary>,
    )

    const dialog = await screen.findByRole('dialog', { name: 'Edit subscription' })
    expect(dialog).toHaveAttribute('aria-modal', 'true')
    expect(dialog).toHaveClass('bg-[var(--surface)]')
    expect(dialog).not.toHaveClass('backdrop-blur-2xl')
    expect(dialog.parentElement).toHaveClass('fixed', 'inset-0')
    expect(dialog.parentElement?.parentElement).toBe(document.body)
    expect(within(dialog).getByText('Modal body')).toBeInTheDocument()
    const okButton = within(dialog).getByRole('button', { name: 'OK' })
    expect(okButton.parentElement).toHaveClass('pt-4')
    expect(okButton.parentElement?.parentElement).toHaveClass('px-6', 'pb-4')

    const mask = dialog.parentElement
    expect(mask).not.toBeNull()
    fireEvent.mouseDown(mask!)
    fireEvent.click(mask!)
    expect(cancel).toHaveBeenCalledOnce()
  })

  it('uses viewport-relative full and almost-full presets', async () => {
    const rendered = render(<Modal open title="Full" size="full" footer={null}>Full body</Modal>)
    const full = await screen.findByRole('dialog', { name: 'Full' })
    expect(full).toHaveStyle({ width: '100%', height: '100%', maxWidth: '100%', borderRadius: 0 })

    rendered.rerender(<Modal open title="Almost full" size="almost-full" footer={null}>Almost full body</Modal>)
    const almostFull = await screen.findByRole('dialog', { name: 'Almost full' })
    expect(almostFull).toHaveStyle({
      width: 'calc(100% - 48px)',
      height: 'calc(100% - 48px)',
      maxWidth: 'calc(100% - 48px)',
    })
    expect(almostFull.parentElement).toHaveClass('items-center', 'overflow-hidden')
  })

  it('keeps default modal content scrollable inside the viewport', async () => {
    render(<Modal open title="Long content" centered>Modal body</Modal>)

    const dialog = await screen.findByRole('dialog', { name: 'Long content' })
    const body = within(dialog).getByText('Modal body')

    expect(dialog).toHaveStyle({ maxHeight: 'calc(100dvh - 32px)' })
    expect(body).toHaveStyle({ minHeight: 0, overflowY: 'auto', overflowX: 'hidden' })
  })

  it('reports closing only after a visible modal completes its exit transition', async () => {
    const afterOpenChange = vi.fn()
    const rendered = render(<Modal open title="Animated" afterOpenChange={afterOpenChange}>Animated body</Modal>)
    const dialog = await screen.findByRole('dialog', { name: 'Animated' })

    expect(afterOpenChange).not.toHaveBeenCalledWith(false)
    rendered.rerender(<Modal open={false} title="Animated" afterOpenChange={afterOpenChange}>Animated body</Modal>)
    await waitFor(() => expect(dialog).toHaveClass('opacity-0'))
    expect(dialog).toBeInTheDocument()
    await waitFor(() => expect(afterOpenChange).toHaveBeenCalledWith(false))
    expect(screen.queryByRole('dialog', { name: 'Animated' })).not.toBeInTheDocument()
  })

  it('dispatches Escape only to the topmost modal', async () => {
    const closeParent = vi.fn()
    const closeChild = vi.fn()
    render(
      <AcmeContentBoundary>
        <Modal open title="Parent" footer={null} onCancel={closeParent}>Parent body</Modal>
        <Modal open title="Child" footer={null} onCancel={closeChild}>Child body</Modal>
      </AcmeContentBoundary>,
    )

    await screen.findByRole('dialog', { name: 'Child' })
    await waitFor(() => expect(screen.getAllByRole('dialog')).toHaveLength(2))
    fireEvent.keyDown(document, { key: 'Escape' })
    expect(closeChild).toHaveBeenCalledOnce()
    expect(closeParent).not.toHaveBeenCalled()
  })
})
