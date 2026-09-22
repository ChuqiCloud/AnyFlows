import { cn } from '@/lib/utils'

/*
 * 认证页共用的外观常量：HeroUI 自带的内边距、字号与间距在这里统一覆盖回原设计值。
 * 圆角无需处理——HeroUI 的 rounded-large / rounded-medium 分别为 14px / 12px，与原设计一致。
 */
export const cardClass = 'border border-[var(--hairline)] bg-card text-card-foreground'

const buttonBase = 'min-w-0 rounded-lg font-medium [&_svg:not([class*=size-])]:size-4'
export const buttonPrimary = cn(buttonBase, 'h-9 gap-1.5 px-3.5 hover:bg-[var(--primary-hover)]')
export const buttonPrimaryLg = cn(buttonBase, 'h-11 min-h-11 gap-2 px-5 text-[0.9375rem] hover:bg-[var(--primary-hover)]')
export const buttonSecondary = cn(buttonBase, 'h-9 gap-1.5 px-3.5 border border-[var(--hairline)] bg-transparent text-foreground hover:bg-surface-2 hover:border-white/15 light:hover:border-black/15')
export const buttonSecondaryLg = cn(buttonBase, 'h-11 min-h-11 gap-2 px-5 text-[0.9375rem] border border-[var(--hairline)] bg-transparent text-foreground hover:bg-surface-2 hover:border-white/15 light:hover:border-black/15')
export const buttonLight = cn(buttonBase, 'h-9 gap-1.5 px-3.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground')
export const buttonLightIconSm = cn(buttonBase, 'size-8 text-muted-foreground hover:bg-surface-2 hover:text-foreground')

/** 圆形图标按钮：登录卡片底部的次要入口（Passkey、企业 SSO）。 */
export const buttonIconRound = cn(buttonBase, 'size-10 min-w-10 rounded-full border border-[var(--hairline)] bg-transparent text-muted-foreground hover:border-white/15 hover:bg-surface-2 hover:text-foreground light:hover:border-black/15')

/*
 * HeroUI Input 比原生 input 多一层包装容器，因此把原来的边框、底色、高度、内边距
 * 移到 inputWrapper，内部 input 只保留排版与占位符配色，外观才与原来一致。
 */
export const inputWrapperClass = 'min-h-11 rounded-lg border border-input bg-transparent px-3 py-1 shadow-none data-[hover=true]:bg-transparent group-data-[focus=true]:bg-transparent focus-within:border-ring focus-within:ring-3 focus-within:ring-ring/50'
export const inputTextClass = 'text-base md:text-sm placeholder:text-muted-foreground'
