import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { AcmeShowcase } from './AcmeShowcase'

describe('AcmeShowcase', () => {
  afterEach(() => cleanup())

  it('renders the copied component catalog', () => {
    render(<AcmeShowcase />)
    expect(screen.getByRole('heading', { name: 'ACME Components' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Inputs and selection' })).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Floating and overlay' })).toBeInTheDocument()
  })
})
