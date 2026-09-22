import { useEffect, useState } from 'react'

export type SiteTheme = 'light' | 'dark'

/* 与 index.html 的首帧脚本、ThemeToggle 共用同一个 key。 */
const STORAGE_KEY = 'anyflows.theme'

function readTheme(): SiteTheme {
  const stored = window.localStorage.getItem(STORAGE_KEY)

  if (stored === 'light' || stored === 'dark') {
    return stored
  }

  // 深色是正统形态，仅在系统明确偏好浅色时才默认浅色。
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

/**
 * 全站唯一的主题来源：html 上的主题类只由这里写。
 *
 * 不要改回 @heroui/use-theme——它写的是另一个 key（heroui-theme），挂载时还会重写
 * html 的主题类，于是带 Navbar 的页面会丢掉站点自己的选择、改成跟随系统偏好，
 * 两套主题状态互相打架。
 */
export function useSiteTheme() {
  const [theme, setTheme] = useState<SiteTheme>(readTheme)

  useEffect(() => {
    const root = document.documentElement

    // 两个类都显式写入：token 定义在 .dark/.light 上，dark: 变体也依赖 .dark。
    root.classList.toggle('dark', theme === 'dark')
    root.classList.toggle('light', theme === 'light')
    window.localStorage.setItem(STORAGE_KEY, theme)
  }, [theme])

  return { theme, setTheme, isDark: theme === 'dark' }
}
