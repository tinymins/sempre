import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'
import { Table, type TableColumn } from './Table'

interface Row {
  id: string
  name: string
  size: number
}

const rows: Row[] = [
  { id: 'z', name: 'Zulu', size: 2 },
  { id: 'a', name: 'Alpha', size: 3 },
  { id: 'm', name: 'Mike', size: 1 },
]

const columns: Array<TableColumn<Row>> = [
  { title: 'Name', dataIndex: 'name', sorter: (left, right) => left.name.localeCompare(right.name) },
  { title: 'Size', dataIndex: 'size', sorter: (left, right) => left.size - right.size },
]

describe('Table sorting', () => {
  afterEach(() => cleanup())

  it('cycles ascending, descending, and original order from a sortable header', () => {
    render(<Table rowKey="id" columns={columns} dataSource={rows} pagination={false} />)
    const header = screen.getByRole('columnheader', { name: 'Name' })

    expect(header).toHaveAttribute('aria-sort', 'none')
    fireEvent.click(header)
    expect(rowNames()).toEqual(['Alpha', 'Mike', 'Zulu'])
    expect(header).toHaveAttribute('aria-sort', 'ascending')

    fireEvent.click(header)
    expect(rowNames()).toEqual(['Zulu', 'Mike', 'Alpha'])
    expect(header).toHaveAttribute('aria-sort', 'descending')

    fireEvent.click(header)
    expect(rowNames()).toEqual(['Zulu', 'Alpha', 'Mike'])
    expect(header).toHaveAttribute('aria-sort', 'none')
  })

  it('supports sorting from the keyboard', () => {
    render(<Table rowKey="id" columns={columns} dataSource={rows} pagination={false} />)
    const header = screen.getByRole('columnheader', { name: 'Size' })

    fireEvent.keyDown(header, { key: 'Enter' })
    expect(rowNames()).toEqual(['Mike', 'Zulu', 'Alpha'])
    expect(header).toHaveAttribute('aria-sort', 'ascending')
  })
})

function rowNames() {
  return screen.getAllByRole('row').slice(1).map((row) => within(row).getAllByRole('cell')[0].textContent)
}
