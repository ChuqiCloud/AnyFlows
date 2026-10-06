import { heroui } from '@heroui/theme'

/*
 * HeroUI 外壳色板。
 *
 * HeroUI 的插件只接受真实色值（内部先用 color() 解析成 HSL 通道再生成 --heroui-* 变量），
 * 传 var(--x) 解析不了，所以这批色值只能在这里再写一遍——它必须与 src/index.css 的
 * .dark / .light token 保持一致。不一致的后果是实打实的：HeroUI 默认色板是
 * 「纯黑底 + 中性锌灰文字 + 天蓝主色」，而业务页是「海军蓝底 + 蓝灰文字 + 靛蓝主色」，
 * 两者并排出现在同一个控制台里。
 *
 * 色阶按 HeroUI 的惯例生成：同一份亮度阶梯，深色主题整体反向，
 * 于是「低号 = 浅表面、高号 = 亮文字」的语义在两个主题下都成立。
 */
const SHADES = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900]
const LADDER = [95, 89.6, 79.2, 68.8, 58.4, 46.7, 38.4, 28.8, 19.2, 9.6]

function toHsl(hex) {
  const [r, g, b] = hex.replace('#', '').match(/../g).map((v) => parseInt(v, 16) / 255)
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const delta = max - min
  const lightness = (max + min) / 2
  const saturation = delta === 0 ? 0 : delta / (1 - Math.abs(2 * lightness - 1))
  const sector = max === r ? (g - b) / delta : max === g ? (b - r) / delta + 2 : (r - g) / delta + 4

  return [Math.round(((sector * 60) % 360 + 360) % 360), Math.round(saturation * 100)]
}

/** 由一个基色展开出 50~900 色阶。 */
function ramp(hex, dark) {
  const [hue, saturation] = toHsl(hex)
  const steps = dark ? [...LADDER].reverse() : LADDER

  return Object.fromEntries(SHADES.map((shade, index) => [shade, `hsl(${hue} ${saturation}% ${steps[index]}%)`]))
}

/*
 * 中性色阶手写而非生成：它决定 text-default-*、bg-default-*、border-default-*，
 * 也就是控制台外壳绝大部分文字与底色，必须和 index.css 的文字层级对齐——
 * 500 是次要文字、600/700 是强调文字、800/900 是正文。
 */
const darkNeutral = {
  50: '#182237',
  100: '#222e42',
  200: '#2d3b52',
  300: '#43516b',
  400: '#677593',
  500: '#9dabc4',
  600: '#b8c3d7',
  700: '#ced7e7',
  800: '#d6deee',
  900: '#e8edf8',
}

const lightNeutral = {
  50: '#eaf0f8',
  100: '#e0e7f1',
  200: '#d2dbe8',
  300: '#bac6d8',
  400: '#8d99b0',
  500: '#52607c',
  600: '#414d68',
  700: '#333e56',
  800: '#262f45',
  900: '#1e2739',
}

/** 把一组强调色展开成 HeroUI 需要的 { DEFAULT, foreground, 50~900 }。 */
function palette(colors, dark) {
  return Object.fromEntries(Object.entries(colors).map(([name, [hex, foreground]]) => [
    name,
    { DEFAULT: hex, foreground, ...ramp(hex, dark) },
  ]))
}

const darkAccents = palette({
  primary: ['#7b90ff', '#101728'],
  success: ['#3aad75', '#06180f'],
  warning: ['#cd9240', '#1b1204'],
  danger: ['#ef6f79', '#1c0609'],
}, true)

const lightAccents = palette({
  primary: ['#3b5cf0', '#ffffff'],
  success: ['#0d7f40', '#ffffff'],
  warning: ['#9c6200', '#ffffff'],
  danger: ['#cd333d', '#ffffff'],
}, false)

export default {
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
    './node_modules/@heroui/theme/dist/**/*.{js,ts,jsx,tsx}',
  ],
  darkMode: 'class',
  plugins: [
    heroui({
      themes: {
        dark: {
          colors: {
            background: '#0e1520',
            foreground: { DEFAULT: '#d6deee', ...darkNeutral },
            // 次级动作在外壳里就是中性表面，所以 secondary 与 default 共用同一组灰阶。
            secondary: { DEFAULT: '#1e2637', foreground: '#d6deee', ...darkNeutral },
            default: { DEFAULT: '#2d3b52', foreground: '#d6deee', ...darkNeutral },
            content1: { DEFAULT: '#161e2d', foreground: '#d6deee' },
            content2: { DEFAULT: '#1e2637', foreground: '#d6deee' },
            content3: { DEFAULT: '#222b3c', foreground: '#d6deee' },
            content4: { DEFAULT: '#28324a', foreground: '#d6deee' },
            /* divider 必须带 alpha：HeroUI 会把它烘进工具类（border-divider → hsl(var(--heroui-divider) / .15)），
               给不透明色值就会生成全不透明的边框，顶栏和侧栏会直接出现白线/黑线。 */
            divider: 'rgba(198, 213, 240, 0.15)',
            focus: '#8ba0ff',
            overlay: '#050810',
            ...darkAccents,
          },
        },
        light: {
          colors: {
            background: '#f2f5fa',
            foreground: { DEFAULT: '#1e2739', ...lightNeutral },
            secondary: { DEFAULT: '#e8ecf4', foreground: '#1e2739', ...lightNeutral },
            default: { DEFAULT: '#d2dbe8', foreground: '#1e2739', ...lightNeutral },
            content1: { DEFAULT: '#fafbfd', foreground: '#1e2739' },
            content2: { DEFAULT: '#eef2f9', foreground: '#1e2739' },
            content3: { DEFAULT: '#e8ecf4', foreground: '#1e2739' },
            content4: { DEFAULT: '#dfe5f0', foreground: '#1e2739' },
            divider: 'rgba(28, 52, 104, 0.15)',
            focus: '#3b5cf0',
            overlay: '#1e2739',
            ...lightAccents,
          },
        },
      },
    }),
  ],
}
