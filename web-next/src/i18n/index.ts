import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

import en from './locales/en.json'
import zh from './locales/zh.json'

const localeKey = 'anyflows.locale'

function getInitialLanguage() {
  const stored = window.localStorage.getItem(localeKey)

  if (stored === 'zh' || stored === 'en') {
    return stored
  }

  return window.navigator.language.toLowerCase().startsWith('zh') ? 'zh' : 'en'
}

void i18n.use(initReactI18next).init({
  fallbackLng: 'zh',
  lng: getInitialLanguage(),
  resources: {
    en: { translation: en },
    zh: { translation: zh },
  },
  interpolation: {
    escapeValue: false,
  },
})

// i18n 切换要同步 HTML lang，便于浏览器和辅助技术选择正确语言规则。
i18n.on('languageChanged', (language) => {
  const normalized = language.startsWith('zh') ? 'zh-CN' : 'en'

  document.documentElement.lang = normalized
  document.title = language.startsWith('zh') ? 'AnyFlows 控制台' : 'AnyFlows Console'
  window.localStorage.setItem(localeKey, language.startsWith('zh') ? 'zh' : 'en')
})

document.documentElement.lang = i18n.language.startsWith('zh') ? 'zh-CN' : 'en'
document.title = i18n.language.startsWith('zh') ? 'AnyFlows 控制台' : 'AnyFlows Console'

export { i18n }
