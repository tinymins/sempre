import {
  Activity, ArrowDown, ArrowRight, ArrowUpRight, BadgeCheck, Box, Check, CircleCheck, Command,
  Copy, createIcons, FileCheck2, FileJson2, GitFork, Languages, Monitor, MonitorDot,
  Moon, Server, ServerCog, ShieldCheck, Sun, Terminal,
} from 'lucide'
import { copy, resolveInitialLocale, type CopyKey, type Locale } from './content'
import { initMotion } from './motion'
import { initTheme } from './theme'
import './styles.css'

const baseCommands = {
  posix: 'curl -fsSL https://sempre.run/install | sh',
  windows: 'irm https://sempre.run/install.ps1 | iex',
} as const

type Platform = keyof typeof baseCommands
type UIMode = '' | 'official' | 'github' | 'url'

let locale = resolveInitialLocale(localStorage.getItem('sempre.site.locale'), navigator.language)
let platform: Platform = 'posix'

function inputValue(selector: string) {
  return document.querySelector<HTMLInputElement>(selector)?.value.trim() ?? ''
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", `'"'"'`)}'`
}

function powershellQuote(value: string) {
  return `'${value.replaceAll("'", "''")}'`
}

function selectedUI() {
  const mode = document.querySelector<HTMLSelectElement>('[data-install-ui-mode]')?.value as UIMode | undefined
  if (mode === 'official') return 'official'
  if (mode === 'github' || mode === 'url') return inputValue('[data-install-ui-source]')
  return ''
}

function currentCommand() {
  const core = inputValue('[data-install-core]')
  const subscription = inputValue('[data-install-subscription]')
  const ui = selectedUI()
  const uiMode = document.querySelector<HTMLSelectElement>('[data-install-ui-mode]')?.value as UIMode | undefined
  const uiDigest = uiMode === 'url' && ui ? inputValue('[data-install-ui-digest]') : ''
  const options = [
    core ? { posix: `--core=${shellQuote(core)}`, windows: `-Core ${powershellQuote(core)}` } : null,
    subscription ? { posix: `--subscription=${shellQuote(subscription)}`, windows: `-Subscription ${powershellQuote(subscription)}` } : null,
    ui ? { posix: `--ui=${shellQuote(ui)}`, windows: `-UI ${powershellQuote(ui)}` } : null,
    uiDigest ? { posix: `--ui-sha256=${shellQuote(uiDigest)}`, windows: `-UISha256 ${powershellQuote(uiDigest)}` } : null,
  ].filter((option): option is { posix: string; windows: string } => option !== null)

  if (options.length === 0) return baseCommands[platform]
  if (platform === 'posix') {
    return `curl -fsSL https://sempre.run/install | sh -s -- ${options.map((option) => option.posix).join(' ')}`
  }
  return `& ([scriptblock]::Create((irm https://sempre.run/install.ps1))) ${options.map((option) => option.windows).join(' ')}`
}

function updateCommand() {
  const output = document.querySelector<HTMLElement>('[data-command-output]')
  if (output) output.textContent = currentCommand()
  const prompt = document.querySelector<HTMLElement>('.command-prompt')
  if (prompt) prompt.textContent = platform === 'windows' ? 'PS>' : '$'
}

function updateUIFields() {
  const mode = document.querySelector<HTMLSelectElement>('[data-install-ui-mode]')?.value as UIMode | undefined
  const sourceField = document.querySelector<HTMLElement>('[data-install-ui-source-field]')
  const digestField = document.querySelector<HTMLElement>('[data-install-ui-digest-field]')
  const sourceInput = document.querySelector<HTMLInputElement>('[data-install-ui-source]')
  if (sourceField) sourceField.hidden = mode !== 'github' && mode !== 'url'
  if (digestField) digestField.hidden = mode !== 'url'
  if (sourceInput) {
    const key = mode === 'url' ? 'uiURLPlaceholder' : 'uiGitHubPlaceholder'
    sourceInput.dataset.i18nPlaceholder = key
    sourceInput.placeholder = copy[locale][key]
  }
  updateCommand()
}

function applyLocale(next: Locale) {
  locale = next
  localStorage.setItem('sempre.site.locale', locale)
  document.documentElement.lang = locale
  document.title = copy[locale].title
  document.querySelectorAll<HTMLElement>('[data-i18n]').forEach((element) => {
    const key = element.dataset.i18n as CopyKey
    element.textContent = copy[locale][key]
  })
  document.querySelectorAll<HTMLElement>('[data-i18n-alt]').forEach((element) => {
    const key = element.dataset.i18nAlt as CopyKey
    element.setAttribute('alt', copy[locale][key])
  })
  document.querySelectorAll<HTMLInputElement>('[data-i18n-placeholder]').forEach((element) => {
    const key = element.dataset.i18nPlaceholder as CopyKey
    element.placeholder = copy[locale][key]
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
  const themeButton = document.querySelector<HTMLElement>('[data-theme-trigger]')
  if (themeButton) {
    themeButton.setAttribute('aria-label', copy[locale].theme)
    themeButton.setAttribute('title', copy[locale].theme)
  }
  const description = document.querySelector<HTMLMetaElement>('meta[name="description"]')
  if (description) description.content = copy[locale].meta
}

function setPlatform(next: Platform) {
  platform = next
  document.querySelectorAll<HTMLButtonElement>('[data-platform]').forEach((button) => {
    button.setAttribute('aria-pressed', String(button.dataset.platform === platform))
  })
  updateCommand()
  const scriptLink = document.querySelector<HTMLAnchorElement>('[data-script-link]')
  if (scriptLink) scriptLink.href = platform === 'windows' ? '/install.ps1' : '/install'
}

async function copyInstallCommand() {
  await navigator.clipboard.writeText(currentCommand())
  const status = document.querySelector<HTMLElement>('[data-copy-status]')
  if (!status) return
  status.textContent = copy[locale].copied
  status.classList.add('is-visible')
  window.setTimeout(() => status.classList.remove('is-visible'), 1800)
}

createIcons({
  icons: {
    Activity, ArrowDown, ArrowRight, ArrowUpRight, BadgeCheck, Box, Check, CircleCheck, Command,
    Copy, FileCheck2, FileJson2, GitFork, Languages, Monitor, MonitorDot, Moon, Server,
    ServerCog, ShieldCheck, Sun, Terminal,
  },
  attrs: { 'aria-hidden': 'true', 'stroke-width': '1.8' },
})
initTheme()
applyLocale(locale)
setPlatform(platform)
updateUIFields()
initMotion()

document.querySelector('[data-language]')?.addEventListener('click', () => {
  applyLocale(locale === 'zh-CN' ? 'en' : 'zh-CN')
})
document.querySelectorAll<HTMLButtonElement>('[data-platform]').forEach((button) => {
  button.addEventListener('click', () => setPlatform(button.dataset.platform as Platform))
})
document.querySelectorAll<HTMLInputElement>('[data-install-core], [data-install-subscription], [data-install-ui-source], [data-install-ui-digest]').forEach((input) => {
  input.addEventListener('input', updateCommand)
})
document.querySelector<HTMLSelectElement>('[data-install-ui-mode]')?.addEventListener('change', updateUIFields)
document.querySelector('[data-copy]')?.addEventListener('click', () => void copyInstallCommand())
