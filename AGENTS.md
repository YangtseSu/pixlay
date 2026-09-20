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

## 验证入口(每轮改动后必须跑)

    cargo fmt --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test
    cargo run -p pixlay-cli -- render --template mosaic-8-s14 --dpi 300 --out /var/tmp/a.jpg

后两条:第二条产出真实图片,你要直接查看它。**看不到图就不要判断渲染对不对。**
(S2 之前没有模板,这条命令从 S2 起有效;S0–S1 用程序化 fixtures 的等价命令。)

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
- **保留中文**:`AGENTS.md`、`docs/*.md` 这类项目内部规划文档(与既有正文一致)。
- 界面文案不在本条内,将来由 i18n 决定。
- *理由:注释与错误信息是要和上游 crate、issue、patch 对话的文本;混语言等于每次都要再翻一遍。*

## 提交纪律

- **每完成一步就 commit 一次**(完成 `docs/STEPS.md` 的一步至少产生一个 commit)。不要攒着多个步骤一起提交。
- **push 必须先拿到用户明确许可。** 未获许可一律只 commit、不 push;不得自行 `git push`、不得改远端配置。
- **commit message 以 `🤖` 结尾**(单独一行)。判据是本次改动**是否含 AI 生成或修改的内容**——
  只要沾了 AI 就带 `🤖`;当前阶段 AI 在写全部代码,所以实际每条都带。
- **message 用英文**(见「语言约定」)。首行格式:`<step>: <what changed>`,例如
  `S2: Freeze template geometry and invariant tests`。非步骤性改动(文档、CI)用 `docs:` / `chore:` 前缀。
- **commit 前必须跑「验证入口」的两条命令**(该命令对本步不适用时,跑本步自己的验证命令);红了不提交。
- 不进库:`target/`(见 `.gitignore`)。进库:`Cargo.lock`(AUR 纪律)。

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

## 不要"改进"的方向

- **不要把 Cairo 换成 GPU 渲染**(wgpu / vello / skia 等)。Cairo 是刻意选择:CPU 无纹理尺寸上限
  (导出可达 139.5MP),且预览/导出天然同源;GTK4 本就依赖 Cairo,打包零成本。本应用 ≤10 张图,
  实测全分辨率合成 40–250ms,GPU 没有收益。
- **不要引入场景图框架来"简化"交互。** 命中测试、层级、撤销栈自己写,
  **这是已知成本,不是遗漏。**
- **不要默认用 mozjpeg。** 默认 libjpeg-turbo 4:4:4,mozjpeg 只作为可选。
  *实测 A1/69.7MP:4590ms/6.79MB vs 475ms/8.50MB——9.7 倍时间换 20% 体积。*
- **不要做模式切换**(编辑模式 / 拼图模式)。用户心智始终是"我在拼图",编辑只是当前选中格子的属性。
- **不做**:美颜、色阶/曲线、在线地理编码、RAW、画笔标记、单图精修模式。
  *理由:色阶/曲线是专业控件且需完整 ICC 管线,目标用户看不懂;地理编码有 API 配额/实名/隐私成本,
  地点文字由用户手填替代;其余与拼图核心价值无关。*

## 模块边界

    pixlay-core     CollageDoc、模板、几何、取景变换、命令历史。禁止依赖 gtk / cairo
    pixlay-imaging  解码(glycin)、重采样、调色、EXIF、色彩空间。禁止依赖 gtk
    pixlay-render   唯一的 draw(doc, target),Cairo + pangocairo。禁止依赖 gtk
    pixlay-cli      无窗口渲染入口,同时是自动化验证工具。禁止依赖 gtk4
    pixlay          gtk4 + libadwaita 外壳与交互

`pixlay-core` 与 `pixlay-imaging` 必须能在无显示器环境下 `cargo test`,保持毫秒级到秒级。
`pixlay-cli` 不得依赖 gtk4——它是快循环,编译时间就是迭代成本。

## 必须存在的不变量

- 模板:格子之间零重叠;并集无内部空洞;cut 类模板面积精确 = 1.0
- 取景:任意 (旋转, 缩放, 位移) 组合下,clamp 后照片覆盖整个格子
- 调色恒等:`factor=1`、`s=1`、`Δ=0` 时输出与输入**逐像素一致**
- 渲染一致性:同构图在 `2N` 与 `N` 两种尺寸下,降采样后 RMSE 低于阈值(实测 2.62/255,阈值取 6)
- 编码:物理尺寸模式必须携带 DPI + ICC,且色度采样与请求一致

**每条能由测试强制的规则只留测试,此文件不重复描述。**

## 待解决 / 待证明

- Cairo 上的 A0@300dpi(9933×14043,139.5MP)已由一次性探针证实可做(2026-09-20):表面 558 MB、
  2 格合成 244 ms、81 次贴图 699 ms、旋转 CJK 文字 16 ms、整程峰值 543 MB、PNG/JPEG/TIFF 均出图。
  **S0 仍需按正式判据复测**,并补一件事:10 格时的峰值内存。(格内照片不覆盖满时露出的背景已定为白底,
  见「硬约束」;S0 只需确认渲染不漏出非白像素。)
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
