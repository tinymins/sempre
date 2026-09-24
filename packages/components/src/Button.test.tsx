import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { Button } from './Button'

describe('Button', () => {
  it('does not apply visual chrome to the unstyled variant', () => {
    render(<Button variant="unstyled">Open</Button>)

    const button = screen.getByRole('button', { name: 'Open' })
    expect(button).toHaveClass('inline-flex')
    expect(button).not.toHaveClass('border', 'h-8', 'px-3', 'rounded-md')
  })
})
