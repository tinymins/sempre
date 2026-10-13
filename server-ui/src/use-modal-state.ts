import { useState } from 'react'

export interface ModalLifecycleProps {
  open: boolean
  afterOpenChange?: (open: boolean) => void
}

export function useModalState<T>(initialValue?: T) {
  const [state, setState] = useState<{ value: T | undefined; open: boolean }>(() => ({
    value: initialValue,
    open: initialValue !== undefined,
  }))

  return {
    ...state,
    show: (value: T) => setState({ value, open: true }),
    close: () => setState(current => ({ ...current, open: false })),
    afterOpenChange: (open: boolean) => {
      if (!open) setState(current => current.open ? current : { value: undefined, open: false })
    },
  }
}
