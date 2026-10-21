# 03 外观系统

外观由四样东西组成：**主题**（颜色）、**字体**、**背景**、**窗口**。每样都有合理的默认值，都可以全局设置，也可以按"配置档"（profile，比如"日常""服务器""演示"）单独覆盖。

具体配色、间距、各区域尺寸见 [08 美术风格与版面](08-art-direction.md)，这一篇讲"系统怎么设计"。

## 1. 主题

### 一个主题文件长什么样

```jsonc
{
  "$schema": "https://…/schema/theme.schema.json",
  "name": "moon-paper",
  "displayName": "月白 Moon Paper",
  "kind": "light",                 // light | dark，决定一些自动处理的方向
  "extends": null,                 // 可以继承另一个主题，只写想改的部分
  "terminal": {                    // 终端区域的 16 色 + 前景/背景/光标/选区
    "background": "#FAF8F3",
    "foreground": "#3A3833",
    "cursor": "#5B6FA8",
    "selectionBackground": "#DCE3F2",
    "black": "#3A3833", "red": "#B23A2E", "…": "…"
  },
  "ui": {                          // 界面颜色（标签栏、面板、按钮……）
    "surface0": "#FAF8F3",         // 最底层：窗口和终端背景
    "surface1": "#F2EFE7",         // 第二层：标签栏、侧栏、面板
    "surface2": "#E9E5DA",         // 第三层：悬停、按下、输入框
    "border": "#DEDACD",
    "text": "#2F2D29",
    "textMuted": "#68645A",
    "accent": "#5B6FA8",
    "accentSoft": "#E3E7F3",
    "onAccent": "#FFFFFF",
    "success": "#3E7330", "warning": "#8A6410", "danger": "#B23A2E"
  },
  "overrides": {                   // 可选：单独改某个组件
    "tab.active.background": "#FFFFFF",
    "agentWidget.background": "#FFFFFFE6"
  }
}
```

内置主题的完整文件在 [`design/themes/`](../design/themes/)。

### 两层颜色

- **基础色**（`terminal` + `ui`）：十几个，普通用户改这些就够了。
- **组件色**（`overrides`）：每个组件的每个状态都有一个名字，比如 `tab.active.background`、`panel.item.hover`。不写就从基础色推导。完整列表放在 JSON Schema 里，编辑器能自动补全。

推导规则举例：

| 组件色 | 默认取值 |
| --- | --- |
| `tab.active.background` | `surface0` |
| `tab.hover.background` | `surface2` |
| `tab.indicator` | `accent` |
| `panel.background` | `surface1`，有背景图时加 85% 不透明度 |
| `panel.item.selected` | `accentSoft` |
| `scrollbar.thumb` | `text` 的 25% 不透明度 |

### 浅色主题的专门处理

浅色终端最常见的问题和对策：

| 问题 | 对策 |
| --- | --- |
| 程序假设背景是黑的，输出白色或黄色文字，在浅色背景上看不见 | ① 主题里的 `white`/`brightWhite` 本身就是中灰色；② 开着 `minimumContrastRatio`，不够清楚的颜色自动加深 |
| 选中文字后看不清 | 选区用浅的强调色，并检查选区上文字的对比度 |
| 光标太淡找不到 | 光标用强调色，不用前景色；可选"光标闪烁时带光晕" |
| 背景图让文字发花 | 背景图默认加一层和底色相同的半透明蒙版（见第 3 节） |
| 长时间看纯白刺眼 | 内置主题都不用纯白 `#FFFFFF` 做底，都带一点暖、冷或粉的色调 |

所有内置主题都要通过 `tools/check_contrast.py` 的检查：正文 ≥ 7:1，普通 ANSI 色 ≥ 4.5:1，次要文字 ≥ 3:1。用户自定义主题在主题编辑器里实时显示对比度，不达标会标黄提醒（不强制）。

### 跟随系统

设置项 `theme` 可以是一个名字，也可以是一对：

```jsonc
"theme": { "light": "moon-paper", "dark": "night-voyage", "follow": "system" }
```

`follow` 可选 `system`（跟随 Windows 浅色/深色设置）、`time`（按日出日落或自定义时间切换）、`off`。

### 导入别人的配色

支持导入：Windows Terminal 的 `schemes`、iTerm2 的 `.itermcolors`、常见的 base16 YAML。导入时只填 `terminal` 部分，`ui` 部分按终端背景色自动生成一套，用户可以再调。

## 2. 字体

```jsonc
"zoom": 1.0,        // 整体缩放：界面和终端一起，Ctrl+= / Ctrl+- / Ctrl+0
"uiScale": 1.0,     // 界面尺寸：只放大标题栏、侧栏、状态栏、面板
"font": {
  "family": "\"Cascadia Code\", \"Cascadia Mono\", Consolas, \"mooncli Symbols\", \"Microsoft YaHei UI\", \"Segoe UI Emoji\", \"Segoe UI Symbol\", monospace, \"Noto Color Emoji\"",
  "size": 14,
  "lineHeight": 1.25,
  "letterSpacing": 0,
  "weight": "normal",
  "weightBold": "bold",
  "ligatures": true,
  "uiFamily": ["Segoe UI Variable Text", "Microsoft YaHei UI"]
}
```

- `family` 是一个 CSS 写法的**回退链**：每个字从前往后找，第一个有这个字的字体负责显示它。
- 默认链的顺序：英文等宽（Cascadia）→ **附带的 Nerd Font 符号字体**（`mooncli Symbols`，提示符美化工具的图标总能显示）→ 中文（微软雅黑）→ 彩色 emoji（Segoe UI Emoji）→ 符号（Segoe UI Symbol）→ 系统等宽字体 → Noto Color Emoji。
  - 附带的符号字体只覆盖私有区（`U+E000–F8FF` 等），不会顶替普通文字。来自 Nerd Fonts 的 Symbols Only 字体，OFL/MIT 协议，许可证随程序放在 `fonts/` 下。
  - Noto Color Emoji 连 ASCII 的数字和 `* #` 都带，放在前面会把提示符里的 `*` 画成 emoji，所以排在系统等宽字体之后。这个问题是实测发现的。
- 字符宽度用 Unicode 15 字素簇规则（`@xterm/addon-unicode-graphemes`）：组合 emoji（👨‍👩‍👧）、国旗、肤色修饰都算一个双宽字符，不会把后面的字挤乱。加载失败时退回 Unicode 11 规则。
- 方框线、块元素、Powerline 箭头由终端自己画（`customGlyphs`），不依赖字体，严丝合缝；比格子宽的字形自动缩小到格子里（`rescaleOverlappingGlyphs`）。
- 推荐用户装 `Maple Mono NF CN` 或 `Sarasa Mono SC`，中英文等宽对齐更好。设置页里有常用字体的快捷按钮和一段混排预览（emoji、箭头、方框线、图标、中文）。

### 缩放：三个互相独立的旋钮

| 设置 | 影响范围 | 怎么调 |
| --- | --- | --- |
| **整体缩放** `zoom` | 界面和终端文字一起 | `Ctrl+=` / `Ctrl+-` / `Ctrl+0`（按物理键位识别，小键盘的 + - 0 也行），档位和浏览器一样：50%…200% |
| **界面尺寸** `uiScale` | 只放大标题栏、侧栏、状态栏、设置页、命令面板、右键菜单 | 设置 → 字体与缩放，80%–160% |
| **终端字号** `font.size` | 只调终端文字 | 在终端里 `Ctrl+滚轮`，或设置页，8–36px |

最终生效值：界面放大倍数 = `zoom × uiScale`，终端字号 = `font.size × zoom`。这样"侧栏和顶部底部栏大、文字小"或者反过来，都能调出来。

实现上，界面部分用 CSS `zoom` 放大（终端区域不放大，xterm.js 在 CSS 缩放的容器里鼠标位置会算错），侧栏占用的屏幕宽度和响应式断点都按放大后的尺寸计算。缩放后在底部弹一个"缩放 110%"的小提示，状态栏显示当前比例。

### 设置页

`Ctrl+,` 或侧栏底部的"设置"打开，分六页：外观（主题、背景图）、字体与缩放、终端（光标、滚动、复制粘贴）、标签栏、Shell 与诊断、快捷键。改动立即生效，300ms 后自动写进 `settings.json`；每组都有"恢复默认"。

- 设置文件写入时先写临时文件再改名，写坏的文件会备份成 `settings.json.broken` 并用默认设置启动。
- Rust 端保存前按自己的结构检查一遍类型，写错了拒绝保存。
- "Shell 与诊断"页列出 pwsh 路径、集成状态、ConPTY 来源、设置文件和历史库位置，可一键复制，方便反馈问题。
- 连字：WebGL 渲染器下的支持情况 M3 验证 **【待核实】**；不支持时设置界面里灰掉并说明原因。

## 3. 背景

背景是 mooncli 外观的重点，分四层从下往上叠：

```
④ 终端文字
③ 蒙版层：和底色同色的半透明层，保证文字清楚（可调 0–100%）
② 背景图层：图片 / 渐变 / 纯色，可模糊、可调亮度
① 窗口材质：Mica / 亚克力 / 不透明
```

```jsonc
"background": {
  "image": "%APPDATA%/mooncli/backgrounds/morning-lake.jpg",
  "fit": "cover",              // cover | contain | tile | center
  "position": "bottom right",  // 九宫格位置
  "opacity": 0.35,             // 图片本身的不透明度
  "blur": 8,                   // 模糊半径，px
  "brightness": 1.05,
  "saturation": 0.9,
  "overlay": 0.6,              // 蒙版层不透明度；0 = 不加蒙版
  "scope": "terminal",         // terminal（只铺终端区）| window（铺满整个窗口，面板半透明）
  "slideshow": { "folder": null, "interval": "30m", "shuffle": true }
}
```

- **按配置档设置**：比如"服务器"配置档用一张偏冷的图，一眼就知道这是连远程的标签页。
- **自动取色**：选好背景图后，可以一键"从图片生成主题"——取图片的主色，生成 `accent` 和三层 `surface`，再自动过一遍对比度检查。
- **内置四张浅色背景**：晨雾远山、月面、浅水波、纸纹。它们不是图片文件，而是按当前主题的颜色**现画**的（`app/src/theme/backgrounds.ts`），所以换主题时背景色调跟着变，也不占安装包体积。
- **导入自己的图片**：设置页里"添加图片"，图片复制到配置目录的 `backgrounds/` 下，之后可以随时切换或删除。
- 性能：背景图解码一次后缓存成 GPU 纹理；模糊在加载时算好，不在每帧算。轮播切换时做 600ms 淡入淡出。

## 4. 窗口

```jsonc
"window": {
  "material": "mica",          // mica | mica-alt | acrylic | none
  "opacity": 1.0,              // material 为 none 时生效
  "titleBar": "integrated",    // integrated（标签融进标题栏）| system
  "cornerStyle": "round",      // round | small | square（Windows 11）
  "padding": [12, 16],         // 终端文字离窗口边缘的距离：上下、左右
  "startupSize": [120, 32],    // 列 × 行
  "rememberPosition": true
}
```

- Mica / 亚克力用 Tauri 2 的窗口特效接口（底层是 `window-vibrancy`）。Windows 10 不支持 Mica，自动退回亚克力或不透明。
- 浅色主题 + Mica 时，`surface0` 会自动加一点透明度，让系统壁纸的颜色隐约透出来，同时保证 ≥ 7:1 的文字对比度。

## 5. 配置档（profile）

```jsonc
"profiles": [
  { "name": "日常", "icon": "terminal", "default": true },
  { "name": "服务器", "icon": "server", "theme": "frost",
    "background": { "image": "…/cold-sea.jpg" },
    "command": "ssh dev-box" },
  { "name": "演示", "font": { "size": 18 }, "background": { "image": null } }
]
```

每个配置档可以覆盖主题、字体、背景、启动命令、起始目录、环境变量。新建标签页时按 `Ctrl+Shift+数字` 直接选配置档。

## 6. 改了马上生效

- 监听 `settings.json` 和主题文件夹，保存后 200ms 内生效，不用重启。
- 文件写错了（JSON 语法错、字段不对）：保留上一份正确的配置继续用，右下角弹出提示，点击直接跳到出错的行。
- 设置界面和文件是同一份数据。在界面上改，会写回文件并保留原来的注释和格式（用 `jsonc-parser` 做局部修改）。

## 7. 主题编辑器

在设置里打开，左边是控件，右边是**实时预览**（一个假的终端，里面固定展示 `git status`、`ls`、报错信息、16 色色板、Claude Code 风格的输出等样例）。

- 每个颜色旁边显示它在背景上的对比度。
- "随机微调"按钮：保持色相关系，整体偏暖 / 偏冷 / 更柔和。
- 导出为文件，或者复制成 JSON 分享。
