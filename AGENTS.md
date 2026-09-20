# Pixlay

Linux 原生拼图工具。2–10 张图;规则与不规则模板;每格独立取景(平移 / 缩放 / ±45° 旋转拉直);
每格调色 + 一键全局统一滤镜;画布级文字层(自由摆放,平铺水印是它的一种模式,支持 `{date}` 等
EXIF 动态字段);导出高分辨率成品图(物理尺寸 + DPI,或指定长边像素)。
GPL-3.0-or-later · Rust · GTK4 + libadwaita 外壳 · Cairo 画布 · 目标平台 Arch/AUR。

**范围判据:主路径最短。** 「选模板 → 放照片 → 调取景 → 导出」必须三分钟内完成。
加任何功能前先问:它是否让主路径变长?会则砍掉。

**已锁定的标识符**

| 项 | 值 |
|---|---|
| 仓库 | `https://github.com/YangtseSu/pixlay`(private) |
| crate | `pixlay` / `pixlay-core` / `pixlay-imaging` / `pixlay-render` / `pixlay-cli` |
| 二进制 | `/usr/bin/pixlay`、`/usr/bin/pixlay-render` |
| 配置 / 工程 | `~/.config/pixlay/` · `.pixlay` |
| 工具链 | edition 2024 · resolver 3 · `rust-version` 跟 Arch 现装 rustc(现 `1.98`);基线数字一律 `--release` 测 |
| app-id | `org.yangtse.Pixlay`(自有域名 `yangtse.org` 反写;不用 `io.github.*` 借来的命名空间) |
| i18n | gettext,域 `pixlay`(源语言英文;`.pot`/`po/` 在仓库根;只用 `xgettext --language=Rust` 抽取) |

## 验证入口(每轮改动后必须跑)

    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test
    cargo run --release -p pixlay-cli -- render --template mosaic-8-s14 --dpi 300 --stats --out /var/tmp/a.jpg

后两条:第二条产出真实图片,你要直接查看它。**看不到图就不要判断渲染对不对。**
(S1 起第二条已有效:`mosaic-8-s14` 由 `pixlay-cli` 的模板登记处提供,`--stats` 让每轮的尺子机器可读;
S2 把它换成生成器产出的固化几何,名字与 `templateVersion` 不变。)

配套口径:

- 产物写磁盘路径(`/var/tmp` 或 `$XDG_CACHE_HOME`),**不要写 `/tmp`**:本机 `/tmp` 是 tmpfs(剩 7.5 GB),
  A0 照片内容 PNG 是 342 MB、TIFF 476 MB,写 tmpfs 等于再吃一份内存。
- "看着对"不算判据。像素级结论(格内是否变、格外是否干净、缝上混色多少)写成探针:
  采样若干坐标点/统计混色像素数,输出数字。
- 峰值内存统一读 `/proc/self/status` 的 `VmHWM`;时间用 `CLOCK_MONOTONIC` 墙钟,合成与编码分开报。
- 测编码性能必须用**非平坦**内容:平坦色块会让 A0 PNG 的体积与时间分别差 78× 与 4.6×。

## 语言约定

- **一律英文**:commit message、代码注释(行内与 doc comment)、标识符、测试名、日志与错误信息、
  配置键名、PKGBUILD 的 `pkgdesc` 等一切面向机器与上游维护者的文本。
  *理由:注释与错误信息是要和上游 crate、issue、patch 对话的文本;混语言等于每次都要再翻一遍。*
- **保留中文**:`AGENTS.md`、`docs/*.md` 这类项目内部规划文档(与既有正文一致)。
- **界面文案只走 i18n,源语言英文**:GUI 文案以英文为 source string;`LANG` 缺失、未知或没有该语言的翻译时
  一律回退英文。翻译是后加的语言包,不在 S7 做。翻译层**只在 `pixlay`**——`pixlay-core` / `-imaging` /
  `-render` / `-cli` 不得依赖任何 i18n 库,它们的错误信息是英文标识性文本,GUI 原样附上即可。
  *理由:core 的错误要与上游 crate 对话,翻译只会让它不可 grep;文案集中在一层才好抽。*
- **CLI 不做多语言**:输出恒为英文,不读 `LANG` / `LC_ALL` / `LANGUAGE`,不按 locale 格式化数字与日期
  (日期一律 ISO 8601);`--json` 的值是稳定英文枚举,不做本地化。
  *理由:CLI 是机器接口,输出随环境变化等于每次解析都要重新猜。*

## 提交纪律

- **每完成一步就 commit 一次**(完成 `docs/STEPS.md` 的一步至少产生一个 commit)。不要攒着多个步骤一起提交。
- **push 必须先拿到用户明确许可。** 未获许可一律只 commit、不 push;不得自行 `git push`、不得改远端配置。
- **commit message 以 `🤖` 结尾**(单独一行)。判据是本次改动**是否含 AI 生成或修改的内容**——
  只要沾了 AI 就带 `🤖`;当前阶段 AI 在写全部代码,所以实际每条都带。
- **message 用英文**(见「语言约定」)。首行格式:`<step>: <what changed>`,例如
  `S2: Freeze template geometry and invariant tests`。非步骤性改动(文档、CI)用 `docs:` / `chore:` 前缀。
- **commit 前必须跑「验证入口」的两条命令**(该命令对本步不适用时,跑本步自己的验证命令);红了不提交。
- 不进库:`target/`(见 `.gitignore`)。进库:`Cargo.lock`(AUR 纪律)。
- **「完成」的判据是进度行已改写并 commit**(见「会话与落盘纪律」),不是代码写完、测试变绿。

## 会话与落盘纪律

**对话不是存储。** 会话会被截断、清空或删除;只存在于对话里的结论等于没发生。
新会话只读文件,不读别人的历史对话;`docs/STEPS.md` 的「当前进度」行是**唯一权威**。

- **一步的完成 = 「当前进度」行已改写 + commit**。测试变绿、数字好看只是必要条件。
  *理由:S1 的契约评审实际已通过,AI 只把缺陷修复与实测 commit 出去、没动进度行;原会话一删,
  闸口就没了——下一个会话照旧停在「下一步:评审契约」,等于评审从未发生。*
- **人工裁定当轮落盘。** 人给出裁定(去留 / 通过 / 否决 / 定阈值 / 定偏离)后,同一轮内写进文件:
  裁定块(日期 + 结论 + 依据)+ 进度行改写,然后 commit。不得「先记着,回头一起写」,
  不得只在回复里复述裁定而不写文件。
  *参考形态:S0 的 `裁定(2026-09-20,人行):Cairo 保留` + 进度行 `S0 — 已完成并通过裁定`。*
- **闸口的收口五条,缺一不算做完**:
  1. 裁定块写进 `docs/STEPS.md` 该步的补丁层;
  2. 「当前进度」行改成「已完成并通过 <裁定> → 下一步 X」;
  3. 「需要人工介入的地方」里那条标注已过或删掉;
  4. 裁定改动的形状同步进 `docs/CONTRACT.md`;
  5. commit。
- **落盘不是「下一步的前置」,是裁定这个动作的另一半。** 裁定之后不得在同一会话里继续做下一步:
  闸口的下一个动作是新会话(见 `docs/STEPS.md` 切分原则 5)。
- **不得凭记忆补写落盘。** 结论依赖只在对话里出现过的数字或裁定、而文件里没有时,重测或向人要;
  凭记忆补写正是切分原则要防的「把自己没落盘的草稿当既定前提」。
- **会话结束前自检**:本轮结论(裁定、阈值、实测数字)是否都在文件里,`git status` 是否干净。
  *理由:会话怎么结束不由 AI 决定,能保证的只有文件里的状态。*

## 版本策略:追新

目标平台是 Arch(滚动),所以**没有理由为旧版本兼容**。一切跟着最新稳定版走。

- **工具链**:跟 Arch 现装的 rustc。`rust-version` 直接写 Arch 的版本(现 `1.98`),不向下兼容;
  Arch 升了就跟着升,不做"为了旧工具链少用一个特性"的妥协。
- **依赖**:只取最新稳定版。不写 `=` / `<=` 上限,不 pin 旧版本;每步开工先 `cargo update --workspace`
  再跑「验证入口」。升级带来的破坏**照修**,不写兼容 shim、不留旧路径(与 clean cutover 同一条规矩)。
- **edition / style edition / resolver**:用当前 stable 支持的最高(现 edition 2024、resolver 3)。
- **系统库与绑定**:GTK / cairo / pango / libadwaita 跟 Arch 系统版(现 gtk4 4.22.5、cairo 1.18.4、
  libadwaita 1.9.4);对应绑定取最新。绑定要求的系统版本高于 Arch 时才降绑定,不降系统。
- **CI / 打包**:容器用 `archlinux:latest`,不钉镜像 tag。
- **依赖保持最少**与追新不冲突:数量少,但每一个都取最新。

这条政策**不进测试**:测试不该依赖网络或工具链版本。它靠两点维持——每步开工的 `cargo update`,
以及 review 时看 `Cargo.lock` 的差异。

## 硬约束

- **几何只用归一化坐标 `[0,1]`。** 绝对像素只允许出现在渲染边界,绝不进 `CollageDoc` 或模板。
  *理由:曾因布局单位与设备像素混用,连续两轮得出错误结论。*
- **预览与导出必须调用同一个 `render::draw(doc, target)`。** 禁止第二个渲染实现。
  *理由:产品就是导出图;两套渲染必然分歧——实测同一段 CJK 文字在两个引擎间度量完全一致、像素差 14.6%。*
- **所有重采样归前级,画布只 blit 与 clip。** 解码、降采样、旋转插值、调色都在 `pixlay-imaging` 完成,
  Cairo 只接收已经是对的大小的位图。*理由:这让 Cairo 的滤镜弱点(仅双线性 + mipmap)不影响成品。*
- **编码与元数据一趟完成。** 色度采样、ICC、DPI 必须在同一条管线内;禁止"先编码再单独补元数据"。
  *理由:第二趟会用默认参数重编码,静默把 4:4:4 降成 4:2:0(实测 2.71MB → 1.49MB)。*
  实现含义:**cairo 只提供像素,元数据由编码器自己写。** 实测 `cairo_surface_write_to_png` 在 A0 上
  只输出 IHDR/bKGD/IDAT——无 pHYs、无 iCCP(`identify` 报 `Units: Undefined`),
  `cairo_surface_set_fallback_resolution` 对位图后端无效。用 cairo 写 PNG 必然丢 DPI。
- **求值顺序冻结**,不得重排:
  `解码+色彩规范化 → 几何(裁切/翻转/90°/任意角旋转) → 每格调色 → 全局滤镜 → 格子合成 → 画布装饰 → 文字层 → 输出变换`
  *理由:改变坐标系的操作必须先执行,画布相对定位的内容层必须后执行。反例:先加水印再旋转,水印会跟着转并被插值糊掉。*
- **合成到不透明白底。** 源图 alpha 与未被照片覆盖的画布/格内区域一律归白,导出永远不透明。
  *理由:产品是照片拼图,预览与导出必须逐像素同源;带 alpha 的导出会再开一条编码分支。*
- **原图只读。** 所有编辑是参数,不是像素覆盖。任何情况下不得写回用户源文件。
- **重采样必须在正确色彩空间完成**:`sRGB → 线性 → 处理 → sRGB`。
  中间缓冲用 **16-bit**,量化放管线末端。*理由:8-bit 中间缓冲会放大色带,在天空/灰墙/阴影处出现可见断层。*
- **模板几何是固化数据**,带 `templateVersion`,生成必须确定性。禁止改变已存工程的版面。
- **取景抽象是「父容器裁剪 + 子图元变换」**,不是"每帧算裁剪矩形再 drawImage"。
  取景状态用**绝对 zoom**(显示宽度 / 画布宽度),不用"相对铺满的倍数"。
  *理由:后者在用户换照片时会跳焦,因为"铺满"的基准随图片长宽比变化。*
- **旋转只裁边,不扩画布。** 旋转角或格子几何变化后**必须重算 clamp**,保证格子仍被填满。
  *理由:拼图格子尺寸由模板定死,这条砍掉了通用编辑器里最麻烦的扩画布分支。*
- **GTK 类型不实现 `Send`/`Sync`。** 后台解码与缩放必须经 channel 回主线程,不得跨线程持有 GTK 对象。
- **`ui` 不得直接操作像素。**

## GNOME HIG(只约束 `pixlay` 外壳)

规范:https://developer.gnome.org/hig/ —— 平台定义就是 GTK4 + libadwaita,与「模块边界」一致。
**只对 GUI 层生效**:`pixlay-core` / `-imaging` / `-render` / `-cli` 没有界面,不受影响,也不得为它引 GTK。
HIG 没有版本号,**不做冻结**;引用 URL 与章节名,改 UI 的每一步开工时重读并更新 `docs/HIG-REVIEW.md`。

- **控件**:默认用 libadwaita 的容器与控件(`AdwApplicationWindow` / `AdwToolbarView` / `AdwHeaderBar` /
  `AdwToast` / `AdwStatusPage` / `AdwAboutDialog` 等,具体集合 S7 定)。自绘控件是例外,要写理由。
- **样式**:只用 libadwaita 的 style class 与 CSS 变量,禁止硬编码颜色与间距(否则深浅色与高对比失效)。
  界面样式**跟随系统**(`AdwStyleManager` 保持默认,不写死 light/dark);v1 不提供每应用样式切换控件。
  画布与导出**恒为不透明白底,与 UI 主题无关**——白底是内容,不是界面样式(见「硬约束」)。
- **键盘**:标准快捷键照 HIG `reference/keyboard`;禁止占用 `Alt+*`、`Super+*` 与系统保留组合;
  只用键盘要能走完主路径,每个 action 都要有键盘路径。
- **可访问性**:每个可交互控件都要有可访问名(HIG `guidelines/accessibility`)。
- **文案**:遵循 HIG `guidelines/writing-style`(句子大小写、不用术语、不用敬语);机制仍走「语言约定」
  的 i18n,不在这里重复。
- **刻意偏离(不修,别当 bug)**:
  - 不做 GNOME Shell 搜索提供者、不做通知工作流、不做手机型布局——与「不做」清单同一条纪律;
  - 不做每应用样式偏好控件(light/dark/system 三选一):会加长主路径,而"跟随系统"已经覆盖用户
    表达"想要深色"的方式;
  - 「大字体」**不得**缩放画布内的文字层:那是文档内容,预览/导出必须逐像素同源;
  - HIG `patterns/containers/selection-mode` **不适用**(没有集合视图与多选批量操作);且该页自身写明
    "编辑就是主要交互时不应有独立编辑模式",与「不要做模式切换」同向,**不是**偏离。
- 能机器判定的部分只写在 S7 的测试里(见 `docs/STEPS.md`);目视部分逐条过 `docs/HIG-REVIEW.md`。

## 不要"改进"的方向

- **不要把 Cairo 换成 GPU 渲染**(wgpu / vello / skia 等)。Cairo 是刻意选择:CPU 无纹理尺寸上限
  (导出可达 139.5MP),且预览/导出天然同源;GTK4 本就依赖 Cairo,打包零成本。本应用 ≤10 张图,
  实测全分辨率合成 40–250ms,GPU 没有收益。
- **不要引入场景图框架来"简化"交互。** 命中测试、层级、撤销栈自己写,
  **这是已知成本,不是遗漏。**
- **不要默认用 mozjpeg。** 默认 libjpeg-turbo 4:4:4,mozjpeg 只作为可选。
  *实测 A1/69.7MP:4590ms/6.79MB vs 475ms/8.50MB——9.7 倍时间换 20% 体积。*
- **不要做模式切换**(编辑模式 / 拼图模式)。用户心智始终是"我在拼图",编辑只是当前选中格子的属性。
  *(HIG 的 `selection-mode` 不适用,且其自身建议与本条同向——见「GNOME HIG」。)*
- **不做**:美颜、色阶/曲线、在线地理编码、RAW、画笔标记、单图精修模式。
  *理由:色阶/曲线是专业控件且需完整 ICC 管线,目标用户看不懂;地理编码有 API 配额/实名/隐私成本,
  地点文字由用户手填替代;其余与拼图核心价值无关。*

## 模块边界

    pixlay-core     CollageDoc、模板、几何、取景变换、命令历史。禁止依赖 gtk / cairo
    pixlay-imaging  解码(glycin)、重采样、调色、EXIF、色彩空间。禁止依赖 gtk
    pixlay-render   唯一的 draw(doc, target),Cairo + pangocairo。禁止依赖 gtk
    pixlay-cli      无窗口渲染入口、自动化验证工具,兼 AI 的操作面。禁止依赖 gtk4
    pixlay          gtk4 + libadwaita 外壳与交互

`pixlay-core` 与 `pixlay-imaging` 必须能在无显示器环境下 `cargo test`,保持毫秒级到秒级。
`pixlay-cli` 不得依赖 gtk4——它是快循环,编译时间就是迭代成本。

**`pixlay-cli` 是唯一的机器操作面。** 任何能力都必须先有子命令:零交互(不读 stdin、不等提示)、
stdout 只放机器可读结果、诊断走 stderr、退出码固定、同输入同输出、**输出不受 locale 影响**。
**禁止只有 GUI 能做而 CLI 做不到的事。**
*理由:模型看不到窗口,只能读 CLI 的 stdout;而"看着对"不算判据——目视结论必须在 CLI 里变成数字(探针)。
契约细节(子命令、字段、退出码)在测试与 `docs/STEPS.md`,此处不重复。*

## 必须存在的不变量

- 模板:格子之间零重叠;并集无内部空洞;cut 类模板面积精确 = 1.0
- 取景:任意 (旋转, 缩放, 位移) 组合下,clamp 后照片覆盖整个格子
- 调色恒等:`factor=1`、`s=1`、`Δ=0` 时输出与输入**逐像素一致**
- 渲染一致性:同构图在 `2N` 与 `N` 两种尺寸下,降采样后 RMSE 低于阈值(实测 2.62/255,阈值取 6)
- 编码:物理尺寸模式必须携带 DPI + ICC,且色度采样与请求一致

**每条能由测试强制的规则只留测试,此文件不重复描述。**

## 待解决 / 待证明

- Cairo 上的 A0@300dpi(9933×14043,139.5MP)**已复测通过**(S0,2026-09-20;正式判据是仓库内探针,
  不是一次性脚本):2 格合成 185 ms、10 格 551 ms、合成峰值 `VmHWM` 941 MB、含编码 1340 MB(预算 2.5 GB)、
  PNG/JPEG 均 9933×14043 出图、白底/缝/文字判据全绿。数字见 `docs/STEPS.md`「S0 结果」。
  **裁定(2026-09-20):Cairo 保留**——「不要换成 GPU 渲染」那条继续生效。
- glycin 在**非 Flatpak** 环境(直接 pacman 安装运行)是否零配置可用,含 HEIC:
  本机 `glycin-thumbnailer`(发行版 2.1.5)**全格式失败**(PNG/JPEG/HEIC/AVIF 均报 `Operation not supported`;
  loader 二进制与 bwrap 都在,带不带 session bus 一样),原因未定位。
  追新政策下用的是 crates.io 的 `glycin` 4,源码实测:`glycin-core` 4.0.0 的 `COMPAT_VERSION = 2`
  → 与 Arch 已装的 `2+` loader 兼容;但 `glycin` facade 在 Linux 上**硬依赖 `glycin-external`**
  (沙箱 loader 进程,要 libseccomp/bwrap/系统 loader 包/D-Bus 连接)。
  自包含替代:`glycin-builtin` + `builtin-image-rs`,进程内 loader,但**不含 HEIC**。
  结论未定 → S4 第一条是两条路各跑一次真实解码再选,且 CLI 与测试要有一条不依赖沙箱 loader 的路。
- Pango 中文避头尾与标点挤压的实际效果。
- 异形格的旋转 clamp 采用外接轴对齐矩形做保守计算(允许轻微留白),是否可接受。
- 细长格子的取景缩放下限可能爆炸(实测极端配置需 6.7–7.6 倍)。策略未定:
  下限超过阈值(建议 1.5×)时应改为**限制旋转角**,而不是无限放大。
- 接缝行为已重测(2026-09-20):相邻格共用边上的混色像素数 / 缝长 ≈ 1.08,A0 与 1/5 尺寸下**完全相同**
  → 混色宽度是 1 物理像素、与输出分辨率无关,无强渗色。**残留问题:低分辨率预览里这条 1px 缝可见,
  导出到 300dpi 时 1px ≈ 0.085 mm 不可见。** 预览与导出同源,这条缝是否接受需定。

## AUR 纪律

- 提交 `Cargo.lock`;依赖保持最少;`cargo vendor` 必须能通过
- PKGBUILD 用 `--frozen --offline`,`depends=('gtk4' 'libadwaita')`
- SPDX 统一 `GPL-3.0-or-later`(不是 `-only`)

## 依赖登记

每个新依赖在这里登记:**名字 / 版本 / 为什么 / 体积与影响**。判据是「删掉它要写多少代码」,
以及它是否把某条硬约束变弱。政策见「版本策略:追新」:只取最新稳定版,不 pin 上限。

| 依赖 | 用于 | 为什么 | 备注 |
|---|---|---|---|
| `serde` + `serde_derive` 1.0.229 | `pixlay-core` | `.pixlay` 是 JSON,`CollageDoc` 的每个字段都要能进能出;手写序列化等于自己实现一遍格式校验 | 体积小,无系统依赖 |
| `serde_json` 1.0.151 | `pixlay-core`, `pixlay-cli`(dev) | JSON 读写;`deny_unknown_fields` 把「字段拼错」变成加载期错误 | 同上 |
| `thiserror` 2.0.20 | `pixlay-core`, `pixlay-render` | core/render 的错误是 typed error(契约的一部分);`anyhow` 只允许出现在 `pixlay-cli` | 纯宏,零运行时 |
| `cairo-rs` 0.22.9 | `pixlay-render` | 唯一的渲染后端;GTK4 本就依赖 cairo,打包零成本 | 系统库 cairo 1.18.4;`png` feature 只在 dev(金标准读写) |
| `image` 0.25.10 | `pixlay-cli` | **临时代打**:S1 要出 PNG/JPEG,S6 才做「像素 + 色度采样 + ICC + DPI 一趟完成」的编码器;去留由 S6 定 | 目前在 S1 CLI 一处使用;S6 后若无人用则删 |

`pangocairo` 曾由 S0 spike 临时依赖,随 spike 与 `cairo-rs/png` 的 spike 用途一并离开;
S5 接文字层时按本表重新登记。
