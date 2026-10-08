import { createContext, useContext, type ReactNode } from 'react'

export type EditorLayout = 'page' | 'dialog'
const LayoutContext = createContext<EditorLayout>('dialog')

export function EditorLayoutProvider({ layout, children }: { layout: EditorLayout; children: ReactNode }) {
  return <LayoutContext.Provider value={layout}>{children}</LayoutContext.Provider>
}

export function useEditorLayout() {
  return useContext(LayoutContext)
}
