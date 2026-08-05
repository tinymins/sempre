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
    expect(within(dialog).getByText('Modal body')).toBeInTheDocument()
    expect(within(dialog).getByRole('button', { name: 'OK' }).parentElement?.parentElement).toHaveClass('border-t')

    const mask = dialog.parentElement
    expect(mask).not.toBeNull()
    fireEvent.mouseDown(mask!)
    fireEvent.click(mask!)
    expect(cancel).toHaveBeenCalledOnce()
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
