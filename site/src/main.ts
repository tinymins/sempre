import {
  Activity, ArrowRight, ArrowUpRight, BadgeCheck, Box, Command, Copy, createIcons,
  FileCheck2, FileJson2, GitFork, Languages, Monitor, MonitorDot, Server, ServerCog,
  ShieldCheck, Terminal,
} from 'lucide'
import { copy, resolveInitialLocale, type CopyKey, type Locale } from './content'
import './styles.css'

const commands = {
  posix: 'curl -fsSL https://sempre.run/install | sh',
  windows: 'irm https://sempre.run/install.ps1 | iex',
} as const

let locale = resolveInitialLocale(localStorage.getItem('sempre.site.locale'), navigator.language)
let platform: keyof typeof commands = 'posix'

function applyLocale(next: Locale) {
  locale = next
  localStorage.setItem('sempre.site.locale', locale)
  document.documentElement.lang = locale
  document.querySelectorAll<HTMLElement>('[data-i18n]').forEach((element) => {
    const key = element.dataset.i18n as CopyKey
    element.textContent = copy[locale][key]
  })
  document.querySelectorAll<HTMLElement>('[data-i18n-alt]').forEach((element) => {
    const key = element.dataset.i18nAlt as CopyKey
    element.setAttribute('alt', copy[locale][key])
  })
  const languageLabel = document.querySelector<HTMLElement>('[data-language-label]')
  if (languageLabel) languageLabel.textContent = locale === 'zh-CN' ? 'EN' : '中文'
  const languageButton = document.querySelector<HTMLElement>('[data-language]')
  if (languageButton) {
    const label = locale === 'zh-CN' ? 'Switch to English' : '切换到中文'
    languageButton.setAttribute('aria-label', label)
    languageButton.setAttribute('title', label)
  }
  const copyButton = document.querySelector<HTMLElement>('[data-copy]')
  if (copyButton) {
    copyButton.setAttribute('aria-label', copy[locale].copyCommand)
    copyButton.setAttribute('title', copy[locale].copyCommand)
  }
  const description = document.querySelector<HTMLMetaElement>('meta[name="description"]')
  if (description) description.content = copy[locale].meta
}

function setPlatform(next: keyof typeof commands) {
  platform = next
  document.querySelectorAll<HTMLButtonElement>('[data-platform]').forEach((button) => {
    button.setAttribute('aria-pressed', String(button.dataset.platform === platform))
  })
  const output = document.querySelector<HTMLElement>('[data-command-output]')
  if (output) output.textContent = commands[platform]
  const scriptLink = document.querySelector<HTMLAnchorElement>('[data-script-link]')
  if (scriptLink) scriptLink.href = platform === 'windows' ? '/install.ps1' : '/install'
}

async function copyInstallCommand() {
  await navigator.clipboard.writeText(commands[platform])
  const status = document.querySelector<HTMLElement>('[data-copy-status]')
  if (!status) return
  status.textContent = copy[locale].copied
  status.classList.add('visible')
  window.setTimeout(() => status.classList.remove('visible'), 1800)
}

createIcons({
  icons: {
    Activity, ArrowRight, ArrowUpRight, BadgeCheck, Box, Command, Copy, FileCheck2,
    FileJson2, GitFork, Languages, Monitor, MonitorDot, Server, ServerCog, ShieldCheck,
    Terminal,
  },
  attrs: { 'aria-hidden': 'true', 'stroke-width': '1.8' },
})
applyLocale(locale)
setPlatform(platform)

document.querySelector('[data-language]')?.addEventListener('click', () => {
  applyLocale(locale === 'zh-CN' ? 'en' : 'zh-CN')
})
document.querySelectorAll<HTMLButtonElement>('[data-platform]').forEach((button) => {
  button.addEventListener('click', () => setPlatform(button.dataset.platform as keyof typeof commands))
})
document.querySelector('[data-copy]')?.addEventListener('click', () => void copyInstallCommand())
