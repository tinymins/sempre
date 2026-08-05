import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { Button, Dropdown, Modal, useUIContext } from '@acme/components'
import { AcmeContentBoundary } from './AcmeContentBoundary'

function ThemeProbe() {
  const { theme } = useUIContext()
  return <span data-testid="acme-theme">{theme}</span>
}

describe('AcmeContentBoundary', () => {
  afterEach(() => cleanup())

  it('tracks the Sempre theme class', async () => {
    document.documentElement.classList.remove('dark')
    render(<AcmeContentBoundary><ThemeProbe /></AcmeContentBoundary>)
    expect(screen.getByTestId('acme-theme')).toHaveTextContent('light')

    document.documentElement.classList.add('dark')
    await waitFor(() => expect(screen.getByTestId('acme-theme')).toHaveTextContent('dark'))
    document.documentElement.classList.remove('dark')
  })

  it('keeps modal and Floating UI portals at the viewport root', async () => {
    const rendered = render(
      <AcmeContentBoundary>
        <Modal open title="Contained modal" footer={null}>Modal body</Modal>
        <Dropdown trigger={['click']} menu={{ items: [{ key: 'edit', label: 'Edit source' }] }}>
          <Button>Open menu</Button>
        </Dropdown>
      </AcmeContentBoundary>,
    )

    expect(rendered.container.querySelector('[data-acme-portal-root]')).toBeNull()
    const dialog = await screen.findByRole('dialog', { name: 'Contained modal' })
    expect(dialog.parentElement?.parentElement).toBe(document.body)

    fireEvent.click(screen.getByRole('button', { name: 'Open menu' }))
    const menuItem = await screen.findByText('Edit source')
    expect(menuItem.closest('[data-floating-ui-portal]')?.parentElement).toBe(document.body)
  })
})
