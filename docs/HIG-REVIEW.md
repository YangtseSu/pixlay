# GNOME HIG 对照清单

`AGENTS.md` 的「GNOME HIG」一节是**硬约束**,本文件是它的执行清单:把 HIG 的每一节落到
「机器判据 / 目视判据 / 不适用」三档,并记清哪些章节**已经逐页读过**、哪些还没读。

- 规范:<https://developer.gnome.org/hig/>(无版本号,**不冻结**;引用 URL 与章节名)
- 本文件最近一次逐页阅读:**2026-09-20**。做 UI 的每一步(S7、S8)开工时先重读相关章节再更新本文件。
- 读一节就把那一节的判据写进上表:能算的进测试(`docs/STEPS.md` 补丁层的「S7 · GNOME HIG」),
  只能看的进「二」。**别攒着**——HIG 会变,攒着等于下次重读。

## 一、已读章节 → 判据

| HIG 章节 | 落点 | 判据 |
|---|---|---|
| `index`(平台定义:GTK4 + libadwaita) | `AGENTS.md` | GUI 只在 `pixlay` crate;其余 crate 不引 GTK |
| `guidelines/ui-styling` | `AGENTS.md` + S7 测试 | 只用 style class / CSS 变量,禁硬编码颜色与间距;跟随系统深色;两种样式下都能启动;**画布像素不随样式变** |
| `guidelines/accessibility` | S7 测试 + 本文件「二」 | 每个可交互控件有可访问名(机器);高对比 / 大字体 / 纯键盘 / 屏幕阅读器 / OSK(目视) |
| `guidelines/keyboard` | S7 测试 | 每个 action 有键盘路径;Tab 顺序覆盖全部控件 |
| `reference/keyboard` | S7 测试 | 加速键表 ⊇ 必需集合,∩ 系统保留集合 = ∅;不绑 `Alt+*` / `Super+*` |
| `patterns/containers`(windows / header-bars / popovers / utility-panes / boxed-lists / grid-views / list-column-views) | S7 设计 | 用 libadwaita 容器;本步选定后把实际用的容器回填到本表 |
| `patterns/feedback`(toasts / banners / dialogs / placeholders / spinners / progress-bars / tooltips / notifications) | S7 设计 | 可撤销的短反馈走 `AdwToast`;破坏性操作走对话框;空状态走 `AdwStatusPage`;进度走进度条而非模态 |
| `patterns/containers/selection-mode` | **不适用** | 没有集合视图与多选批量操作;该页自身写明"编辑是主要交互时不应有独立编辑模式",与「不要做模式切换」同向 |

## 二、目视步骤

HIG `guidelines/accessibility` 的 "Testing for Accessibility" 逐条照做。**每条的第二步都是本产品的附加判据**:
画布是**内容**,界面是**样式**,两者不得互相影响(见 `AGENTS.md`「合成到不透明白底」)。

1. **高对比模式**(GTK Inspector 或系统无障碍设置):界面元素全部正常渲染;画布像素不变。
2. **大字体**(系统无障碍设置):标签全部可读、不被截断;画布像素不变(文字层是文档内容,不随字号缩放)。
3. **纯键盘**:只用键盘走完「选模板 → 放照片 → 调取景 → 导出」;焦点顺序合乎逻辑;
   `F10` 开菜单、`Esc` 关浮层、`Tab` 覆盖全部控件。
4. **屏幕阅读器**:每个控件都被读出,可访问名准确、简短;关掉显示器仍能操作。
5. **触摸 / 屏上键盘(OSK)**:文字层内容与导出路径能被 OSK 输完。
6. **S7 另加**:「三分钟主路径」计时(脚本化步骤清单 + 计时,见 `docs/STEPS.md`)与文案措辞
   (`guidelines/writing-style` 的句子大小写、不用术语、不用敬语;漏包装不做机器判据)。

## 三、刻意偏离(`AGENTS.md` 同一份,不修)

| HIG 条目 | 决定 | 理由 |
|---|---|---|
| GNOME Shell 搜索提供者、通知工作流 | 不做 | 与「不做」清单同一条纪律:不服务于主路径的功能不加 |
| 手机型布局 | 不做 | 目标平台是 Arch 桌面;画布有物理尺寸语义 |
| 每应用样式偏好(light / dark / system 三选一) | 不做 | 主路径最短;「跟随系统」已经覆盖用户表达"想要深色"的方式 |
| 大字体模式作用于画布文字层 | 不作用 | 预览与导出必须逐像素同源;文字层是文档内容 |
| access key(`Alt+` 助记符) | 不做 | 本应用没有菜单栏 |

## 四、尚未逐页读的章节(S7 / S8 开工时补读并回填「一」)

- `principles`、`resources`
- `guidelines`:`app-naming`(S8)、`app-icons`(S8)、`ui-icons`(S7)、`writing-style`(S7)、
  `typography`(S7)、`navigation`(S7)、`pointer-touch`(S7)、`adaptive`(S7)
- `patterns/nav`、`patterns/controls/*`
- `patterns/containers/*` 与 `patterns/feedback/*` 的**各页细节**(本表目前只用了它们的索引页)
- `reference/` 里的 UI colors
