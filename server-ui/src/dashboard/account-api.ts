import { serverRequest, type ServerUser } from '../server-api'

export interface AccountSettings {
  langMode?: 'auto' | 'zh-CN' | 'en-US' | 'de-DE' | 'ja-JP' | 'zh-TW'
  themeMode?: 'auto' | 'light' | 'dark'
  accentColor?: 'emerald' | 'amber' | 'rose' | 'violet' | 'blue' | 'cyan'
  avatarKey?: string | null
}

export const accountApi = {
  profile: async () => (await serverRequest<{ user: ServerUser }>('/account/profile')).user,
  update: async (body: { name?: string; email?: string; settings?: Pick<AccountSettings, 'langMode' | 'themeMode' | 'accentColor'> }) =>
    (await serverRequest<{ user: ServerUser }>('/account/profile', { method: 'PATCH', body: JSON.stringify(body) })).user,
  uploadAvatar: (file: File) => {
    const form = new FormData()
    form.append('file', file)
    return serverRequest<{ avatarKey: string; avatarUrl: string }>('/account/avatar', { method: 'POST', body: form })
  },
  deleteAvatar: () => serverRequest<void>('/account/avatar', { method: 'DELETE' }),
  changePassword: (currentPassword: string, newPassword: string) => serverRequest<void>('/account/password', { method: 'POST', body: JSON.stringify({ currentPassword, newPassword }) }),
}
