import {
  Activity,
  ArrowDown,
  ArrowRight,
  Box,
  Check,
  CircleCheck,
  Command,
  Copy,
  FileCheck2,
  GitFork,
  Languages,
  Monitor,
  Moon,
  Network,
  Server,
  ServerCog,
  ShieldCheck,
  Sun,
  Zap,
  createIcons,
} from 'lucide'
import { initMotion } from '../motion'
import { initTheme } from '../theme'

type Locale = 'en' | 'zh-CN'
type Platform = 'posix' | 'windows'

const commands: Record<Platform, string> = {
  posix: 'curl -fsSL https://sempre.run/install | sh',
  windows: 'irm https://sempre.run/install.ps1 | iex',
}

const root = document.documentElement
let locale: Locale = localStorage.getItem('sempre.prototype.locale') === 'zh-CN' ? 'zh-CN' : 'en'
let platform: Platform = 'posix'

function localizedValue(element: HTMLElement, nextLocale: Locale) {
  return nextLocale === 'zh-CN' ? element.dataset.zh : element.dataset.en
}

function applyLocale(nextLocale: Locale) {
  locale = nextLocale
  localStorage.setItem('sempre.prototype.locale', locale)
  root.lang = locale
  document.querySelectorAll<HTMLElement>('[data-en][data-zh]').forEach((element) => {
    const value = localizedValue(element, locale)
    if (value) element.textContent = value
  })
  document.querySelectorAll<HTMLElement>('[data-en-alt][data-zh-alt]').forEach((element) => {
    const value = locale === 'zh-CN' ? element.dataset.zhAlt : element.dataset.enAlt
    if (value) element.setAttribute('alt', value)
  })
  document.querySelectorAll<HTMLElement>('[data-language-label]').forEach((element) => {
    element.textContent = locale === 'zh-CN' ? 'EN' : '中文'
  })
  document.querySelectorAll<HTMLElement>('[data-language]').forEach((element) => {
    const label = locale === 'zh-CN' ? 'Switch to English' : '切换到中文'
    element.setAttribute('aria-label', label)
    element.setAttribute('title', label)
  })
  document.querySelectorAll<HTMLElement>('[data-theme-trigger]').forEach((element) => {
    const label = locale === 'zh-CN' ? '主题' : 'Theme'
    element.setAttribute('aria-label', label)
    element.setAttribute('title', label)
  })
}

function applyPlatform(nextPlatform: Platform) {
  platform = nextPlatform
  document.querySelectorAll<HTMLButtonElement>('[data-platform]').forEach((button) => {
    button.setAttribute('aria-pressed', String(button.dataset.platform === platform))
  })
  document.querySelectorAll<HTMLElement>('[data-command-output]').forEach((element) => {
    element.textContent = commands[platform]
  })
  document.querySelectorAll<HTMLAnchorElement>('[data-script-link]').forEach((element) => {
    element.href = platform === 'windows' ? '/install.ps1' : '/install'
  })
}

async function copyInstallCommand() {
  await navigator.clipboard.writeText(commands[platform])
  document.querySelectorAll<HTMLElement>('[data-copy-status]').forEach((status) => {
    status.textContent = locale === 'zh-CN' ? '安装命令已复制' : 'Install command copied'
    status.classList.add('is-visible')
    window.setTimeout(() => status.classList.remove('is-visible'), 1800)
  })
}

createIcons({
  icons: {
    Activity,
    ArrowDown,
    ArrowRight,
    Box,
    Check,
    CircleCheck,
    Command,
    Copy,
    FileCheck2,
    GitFork,
    Languages,
    Monitor,
    Moon,
    Network,
    Server,
    ServerCog,
    ShieldCheck,
    Sun,
    Zap,
  },
  attrs: { 'aria-hidden': 'true', 'stroke-width': '1.7' },
})

initTheme()
applyLocale(locale)
applyPlatform(platform)
initMotion()

document.querySelectorAll('[data-language]').forEach((element) => {
  element.addEventListener('click', () => applyLocale(locale === 'zh-CN' ? 'en' : 'zh-CN'))
})
document.querySelectorAll<HTMLButtonElement>('[data-platform]').forEach((button) => {
  button.addEventListener('click', () => applyPlatform(button.dataset.platform as Platform))
})
document.querySelectorAll('[data-copy]').forEach((element) => {
  element.addEventListener('click', () => void copyInstallCommand())
})
