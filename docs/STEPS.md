# Pixlay 实施步骤

按顺序执行。**一次只做一步**,做完一步再开下一步。

给新会话的第一句话:

> 读 `AGENTS.md`,再读 `docs/STEPS.md`。只做「当前进度」指向的那一步,不要提前做后面的。
> 每步做完先跑该步的验证命令,把出口判据变成测试,然后更新本文件的「当前进度」。

- **当前进度: S0 — 已完成(2026-09-20,全部判据机器化并全绿;见「S0 结果」),等人工裁定 Cairo 去留 → 下一步 S1**
- 硬约束与不变量在 `AGENTS.md`,本文件只排顺序与出口判据,不重复其内容。
- 每一步开始前先跑一次 `cargo test`,确认基线是绿的。
- **文末《审查补充》是本文件的补丁层**:每步新增的机器判据、待决策项、实测基线都在那里;
  执行某一步时同时读该步在补丁层里的条目。

## 切分原则

1. 每步都必须有一个**能机器判定**的出口。没有可判定出口的"步骤"不是步骤。
2. 先做能推翻整个选型的那一件事(S0)。
3. 契约先冻结,再上量(S1)。
4. **GUI 放最后(S7)**——S0–S6 全部在无窗口、无截图的快循环里完成。
5. **会话边界对齐「闸口」,不对齐步骤数。** 一步做完不必换会话,但这三种情况**必须**停:
   该步结尾有**人工判据或人工决策**(S0 的 Cairo 去留、S1 的契约评审、各步的目视质量、S7 的三分钟主路径);
   该步产出**不可逆的契约或固化数据**(S1 的 `CollageDoc` 形状、S2 的 `templateVersion`、S4 的解码后端
   决定 S8 的 `depends`);该步**可能推翻前序选型**。
   *理由:同一个会话里,模型会把自己没落盘的草稿当成既定前提继续盖楼;契约评审要由"没写过这份草稿"的会话
   执行才有意义。*
   *前提:边界成立的条件是**结论已落盘**(测试里的阈值常量 + 本文件补丁层的实测数字 + 「当前进度」行)。
   没落盘的结论换会话等于重测一遍。*
   按此规则的天然边界是 `S0 ┊ S1 ┊ S2+S3 ┊ S4 ┊ S5+S6 ┊ S7 ┊ S8`(六个会话,不是九个)。

## 需要人工介入的地方

- **S0 结束后**:看数字,决定 Cairo 能不能用。
- **S1 结束后**:评审契约。这是唯一必须由人确认的地方——契约错了后面七步全废,而模型自己看不出"这个契约将来会不够用"。
  评审前先看「五、待决策」:那张表里的每一条都会改变契约形状。

其余步骤模型的自动闭环即可完成,除了**目视判据**(能算的已在「三」里换成可算量,剩下这些只能看):
S0 的文字可读性、S4 的降采样质量、S5 的避头尾与标点挤压手感、S7 的"三分钟主路径"与界面文案有无线漏包装。

---

## S0 · Cairo 极限 spike

**性质:可抛弃。** 这一步允许脏、允许硬编码、允许一次性脚本。

- **目标**:证明 Cairo 能渲出 A0@300dpi。
- **做什么**:一个硬编码的 `[[bin]]`,用 `cairo::ImageSurface` 建 9933×14043 表面,画一条不规则多边形 clip,贴一张带仿射变换的照片,叠一行旋转的 CJK 文字,写出 PNG 与 JPEG。
- **出口判据**:
  - 输出文件存在,`identify` 尺寸为 9933×14043
  - 合成耗时、峰值内存有具体数字
  - 无崩溃、无显存/内存耗尽
  - 照片在格子内、格子外无渗色、文字可读
- **不做**:不做 UI、不做模板库、不做取景数学、不做工程文件。
- **已做(2026-09-20)**:实现是 `crates/pixlay-cli/src/bin/a0-spike.rs`(可抛弃,一次性的硬编码 `[[bin]]`),
  判据、数字与给 S1 的交接见「S0 结果」;`cargo test` 里有 5 条测试在 1/4 A0 尺寸上跑同一套探针。

## S1 · 冻结最小契约 + 反馈回路

- **目标**:让 CLI 闭环成立,后续所有步骤都在它上面做。
- **做什么**:
  - `pixlay-core`: `CollageDoc` / `CanvasSpec` / `Template` / `Slot` / `TextLayer` / `CropTransform` v1,带 serde 与 `templateVersion`
  - `pixlay-render`: 唯一的 `draw(doc, target)`
  - `pixlay-cli`: `render --project x.pixlay --dpi 300 --out y.jpg`
  - fixtures:6 张测试图(EXIF Orientation=6、带 alpha 的 PNG、竖构图、横构图、4:3、方图)
  - 金标准像素测试
- **出口判据**:
  - serde 往返:序列化再反序列化后逐字段相等
  - `pixlay-cli render` 能读 `.pixlay` 出图
  - CLI 的机器面成立:无 TTY、无 stdin 时命令照常成功;stdout 只有机器可读结果;退出码符合契约;
    `LANG` 变化不改变 stdout/stderr 文本
  - `probe` 能用数字回答"格内是否变、格外是否干净、缝上混色多少"
  - 把像素阈值收紧到不可能通过时,测试**必须变红**(验证测试真的在校验)
  - `pixlay-core` 与 `pixlay-render` 的 `cargo test` 不需要显示器
- **不做**:不做 GUI、不做多模板、不做调色、不做文字排版。
- **人工**:契约评审。

## S2 · 模板系统(只有几何)

- **目标**:规则与不规则模板的几何全部正确且可校验。
- **做什么**:异形格用 SVG path;规则格可用 grid span 参数化;**生成必须确定性**,结果固化并带 `templateVersion`。
- **出口判据**:
  - 格子之间**零重叠**
  - 并集**无内部空洞**
  - 解析出的路径面积与声明面积一致(容差内)
  - 同一 `templateVersion` 重复生成结果逐位相同
  - 覆盖 2–10 格各至少一个模板
- **不做**:不引入任何图片;不做渲染。

## S3 · 取景与 clamp

- **目标**:格内取景数学完全正确,含任意角度旋转。
- **做什么**:绝对 zoom(显示宽度 / 画布宽度)、位移、旋转;「父容器裁剪 + 子图元变换」;旋转或格子变化后重算 clamp。可用假图片尺寸,不需要图像管线。
- **出口判据**:
  - 扫掠 (旋转 × 缩放 × 位移 × 每类格子形状),clamp 后照片**恒覆盖整个格子**
  - 旋转角改变触发 clamp 重算,有测试覆盖
  - 只裁边、不扩画布:画布尺寸在任意取景下不变
- **不做**:不做 GUI 手势;不做图像解码。

## S4 · 图像管线

- **目标**:进入渲染的位图在解码、方向、色彩、位深上都是对的。
- **做什么**:`pixlay-imaging`——glycin 解码(含 HEIC/AVIF)、EXIF 自动旋转、重采样(`sRGB → 线性 → 处理 → sRGB`,Lanczos3)、16-bit 中间缓冲、每格调色 + 全局统一滤镜。后台工作经 channel 回主线程的结构先定好。
- **出口判据**:
  - **调色恒等**:`factor=1, s=1, Δ=0` 时输出与输入逐像素一致
  - HEIC 能解;EXIF Orientation=6 自动转正
  - 大幅降采样(4000px → 400px)质量目视无锯齿、无糊
  - 中间缓冲是 16-bit,量化只发生在管线末端
- **不做**:不做调色的 UI。

## S5 · 文字层

- **目标**:画布级文字正确,两种形态走同一套机制。
- **做什么**:文字层为基本单位,平铺水印是它的一种模式;`{date}` 等 EXIF 动态字段;Pango + pangocairo 排版。
- **出口判据**:
  - 自由摆放与平铺水印都能出图
  - `{date}` 从 EXIF 填充正确;**EXIF 缺失时回退行为有测试**
  - CJK 避头尾与标点挤压目视正确
  - 文字在旋转后位置稳定(验证「内容层后执行」这条约束)
- **不做**:不做文字编辑 UI。

## S6 · 导出

- **目标**:两种导出模式都正确,元数据一趟完成。
- **做什么**:物理尺寸 + DPI 模式;指定长边像素模式;编码与元数据在同一条管线内。
- **出口判据**:
  - 物理尺寸模式:文件带正确 DPI 与 ICC
  - 长边像素模式:输出长边像素与请求精确一致
  - 色度采样与请求一致(专抓两趟元数据陷阱)
  - A0 能出 PNG / JPEG / TIFF
- **不做**:不做导出 UI;不做 PDF(未列入范围)。

## S7 · GTK 外壳与交互

**从这里开始才出现窗口。** 底下那一层已被 S1–S6 锁死,所以本步出的问题只可能在 GUI 里。

- **目标**:主路径三分钟内走通。
- **做什么**:gtk4 + libadwaita 外壳;拖入照片;格内拖动/滚轮/双击;参考线拉直;文字层编辑;撤销重做(命令历史 + 轻量 AST 快照);命中测试;后台解码回主线程;**文案走 i18n**(英文为 source string,本步不产出任何翻译)。
- **出口判据**:
  - 「选模板 → 放照片 → 调取景 → 导出」端到端可走通
  - `LANG` 未设、`LANG=C`、`LANG=<未知语言>` 三种情况下界面都是英文且能启动
  - `po/POTFILES` 收录的文件集合 == `crates/pixlay/src/**/*.rs`(可机器比对),`.pot` 随仓库提交;
    是否有文案漏了包装靠本步的目视判据兜(见「需要人工介入」)
  - Wayland 下正常,无阻塞 UI(解码/缩放在后台,主线程不卡)
  - 窗口渲染的画面与 `pixlay-cli render` 同一画布尺寸下像素一致(RMSE 阈值内)
- **不做**:不做模式切换;不做额外面板;不做翻译(语言包是后加的,本步只保证可抽取、可回退)。

## S8 · 打包

- **目标**:能进 AUR。
- **做什么**:`PKGBUILD`、desktop 文件、图标、依赖清单;`po/` 下的翻译与 `.desktop` / metainfo 的多语言字段。
- **出口判据**:
  - `cargo vendor` 通过,`cargo build --frozen --offline` 通过
  - `makepkg` 在干净 chroot 里成功
  - 桌面文件与图标通过校验
  - 装出的包能启动并跑通主路径
  - `msgfmt --check` 通过;无语言包时界面是英文(chroot 里通常没有 locale,这条正好被它覆盖)
- **不做**:不做 Flatpak / Snap / 其他发行版。

---

## 审查补充 2026-09-20

来自一次文档审查 + 一次性探针(未进仓库,在 `/var/tmp/pixlay-spike/`)。原始数字见「六、实测基线」。

### 一、文档之间的冲突(需裁定)

1. **验证入口的 CLI 面与 S1 不一致。** `AGENTS.md` 用 `render --template mosaic-8-s14 --dpi 300 --out …`,
   S1 定义的是 `render --project x.pixlay --dpi 300 --out y.jpg`。两个都要有:
   `--template <name>`(无工程无照片,smoke 用)与 `--project <file>`(带照片)。S1 契约必须同时写这两面,
   否则 `AGENTS.md` 那条"每轮必须跑"的命令要到 S2 才跑得动。
2. **入口命令依赖 S2 的产物。** `mosaic-8-s14` 到 S2 才存在。裁定:S2 出口判据点名该模板
   (8 格、不规则、`templateVersion` 固定、生成器可复现);S2 之前把该命令视为"S2 起有效"。
3. **S1 的 fixtures 混入了 S4 的关注点。** "EXIF Orientation=6、带 alpha 的 PNG" 需要解码器与 EXIF 解析。
   裁定:S1 只**提交**这些文件(数据便宜、离线可用),**使用**它们的测试归 S4;S1 的金标准测试用程序化生成的位图。
4. **`io.github.<user>.Pixlay` 里的 `<user>` 未定。** 必须在 S7 写代码前定死,否则 S8 的 app-id 与 desktop 文件返工。

### 二、新增步骤:S6.5 · 命令历史 / 工程 IO / 命中测试(仍无窗口)

S7 里写着「撤销重做(命令历史 + AST 快照)」与「命中测试」。这两件都是纯 `pixlay-core` 逻辑、可无窗口测试,
却又是 GUI 里最容易错的部分;放进 S7 同时破坏两条切分原则(S7 才是最后一个窗口步骤;可机器判定的东西不该等到那时)。

- **做什么**:`Command` + 撤销栈;`.pixlay` 保存/加载 + 版本迁移;点 → 格子命中测试(含旋转与异形格)。
- **出口判据**:
  - 任意操作序列连续 undo 回到初始态后,`draw` 出的像素与初始态**逐像素相同**;redo 同构
  - 保存 → 加载 → 再保存 逐字节相同;缺失文件/坏版本必须给明确错误 + 非零退出码
  - 命中测试扫掠(每模板 × 每格质心 × 每格边界外侧 1px),结果与几何解析解一致
- **不做**:不做 UI 事件绑定,不做手势。

### 三、各步出口判据的机器判定化(补充)

原判据里"目视/可读/三分钟"这类,要么换成可算量,要么明确列为人工判据(已并入「需要人工介入」一节)。

**S0**(选型风险已基本退掉,见「实测基线」;建议本步改为"复现基线 + 补 10 格峰值内存")
- 峰值内存统一 `VmHWM`(见「四、度量口径」),A0 合成峰值 ≤ 2.5 GB
- "格内照片、格外无渗色" → 探针像素:格内采样点等于该格照片色;格外采样点等于画布背景色;
  相邻格共用边上的混色像素数 ≤ 2 × 缝长
- "文字可读" → 文字包围盒内墨色像素占比落在区间内
- **测试内容必须非平坦**:平坦色块会同时低估编码时间与体积(实测差 4.6× / 78×)
- 补一条:画布与未被照片覆盖处一律**白底**(已定,见 AGENTS「硬约束」);S0 只需确认渲染不漏出非白像素——
  探针在格内未覆盖区、格外的画布区各取几点,断言就是纯白

**S0 结果** `[2026-09-20 实测;代码 `crates/pixlay-cli/src/bin/a0-spike.rs`(可抛弃,进仓库只为可复现)]`

上面每一条判据都进了那个 bin 的探针,不再有"看着对"这一步。跑法(release):

    cargo build --release -p pixlay-cli
    ./target/release/a0-spike --content flat   --slots 2  --skip-encode --out /var/tmp/pixlay-s0/flat2
    ./target/release/a0-spike --content flat   --slots 10 --skip-encode --out /var/tmp/pixlay-s0/flat10
    ./target/release/a0-spike --content detail --slots 2  --out /var/tmp/pixlay-s0/detail2  --preview-px 1400
    ./target/release/a0-spike --content detail --slots 10 --out /var/tmp/pixlay-s0/detail10 --preview-px 1400

四条全部 `verdict = ok`。断言与实测(阈值常量的来源都写在代码注释里):

| 判据 | 实测(A0) | 阈值 |
|---|---|---|
| 输出尺寸 | PNG/JPEG 均 9933×14043 | 精确相等 |
| 合成耗时 | 2 格 185 ms · 10 格 551 ms | 无 |
| 合成峰值内存 `VmHWM` | 941 MB(2 格与 10 格相同) | ≤ 2560 MB |
| 整程峰值 `VmHWM`(含编码) | 1340 MB | 无 |
| 格外非白采样 | 0 / 136584(全白) | = 0 |
| 格内未覆盖区(转过 8° 的格角、下移 4% 的条) | 全纯白 | 纯白 |
| 缝混色 | 1.000 px/行,最宽 1 px,三层凸组合残差 0.20/255,0 个解释不了 | ≤ 2 px/行,最宽 ≤ 2 px |
| 文字墨色比 | 0.1148(bbox 3637×610 px,字号 281 px) | 0.02–0.60 |
| 内容非平坦(渲染后逐像素亮度步长) | 3.61(detail 2 格)/ 4.60(detail 10 格);flat 为 0.00 | > 2.0 |

人工判据(照片在格内、格外无渗色、文字可读):2 格与 10 格的 1400 px 预览已逐张目视确认——
不规则 L 形 clip 正确、旋转格的照片被 clip 住不越界、未覆盖处露白、CJK 字形完整可读、缝是 1 px 硬边无渗色。

给 S1 的交接(3 个静默坑,都是本步踩出来的):

1. `cairo::SurfacePattern::set_matrix` 收的是 **用户空间 → pattern 空间**的矩阵:得把"放置矩阵"求逆再交进去。
   方向写反时 cairo 不报错,只是**什么都不画**。
2. `pango::FontDescription::set_absolute_size` 的单位是**像素 × `PANGO_SCALE`(1024)**。少乘这一下
   得到 0.07 px 的字,同样静默(画了,看不见)。
3. 缝的判据不能写成"位于两色之间":与**白底**混合的缝像素本来就会跑到两色的区间外(实测红/蓝缝
   是 138,108,183,绿通道高过两侧)。改成"白 + 左格 + 右格的三层凸组合残差"才有判别力。

依赖(在 S1 的「依赖登记」里一并结算):`cairo-rs 0.22.9` + `pangocairo 0.22.9`(归 `pixlay-render`,长期)、
`image 0.25.10`(S0 只用它的 JPEG 编码与尺寸读取,临时;去留由 S4/S6 定)。spike 删除时 `image` 也应一起走,
除非那时已经有别的用途。

**S1**
- **CLI 是 AI 的唯一操作面**(见 AGENTS「模块边界」),所以它的契约按机器面写,与 `render::draw` 的契约同级冻结:
  - 每条子命令都支持 `--json`;stdout **只**出机器可读结果,诊断/进度/警告走 stderr;同输入同输出
    (结果里不写时间戳、不写绝对路径)
  - **零交互**:不读 stdin、不等提示、无 TTY 时行为不变;`--help` 覆盖全部 flag 与退出码
  - **不受 locale 影响**:stdout/stderr 文本在 `LANG` / `LC_ALL` / `LANGUAGE` 变化下**逐字节相同**(含错误分支)。
    渲染出的像素不在此约束内——文字层的字形回退确实受 locale 影响,所以测试仍固定 `LANG`(见下)
  - `--stats` 输出 `{ms, peak_rss, out_w, out_h, dpi, icc}`,口径照「四、度量口径」,之后每步复用同一把尺子
  - `probe`:采样若干坐标点并输出数字(格内照片色、格外白底、相邻格共用边上的混色像素数);
    AGENTS 的"像素级结论写成探针"由它承担,后续各步不再各写一份一次性脚本
  - `--preview-px <n>`:同一个 `draw` 的缩放目标,产出可直接目视的预览(A0 无法整体目视)
  - 退出码:0 成功 / 1 用法错 / 2 解码或渲染失败;失败时 stderr 打印缺失文件路径,stdout 保持为空
  - **v1 非目标**(与 AGENTS「不做」同一条纪律):MCP server、REPL / watch、自然语言参数、
    从配置文件读默认值因而改变行为——都不做
- `--stats` 落地后,把 AGENTS「验证入口」第二条命令换成带 `--stats` 的形式:每轮的尺子因此是机器可读的
- "阈值收紧到不可能通过时测试必须变红" → 改成**永久自检**:把金标准图在内存里加一个 > 阈值的扰动,
  断言比较函数返回失败。一次性手改没有回归价值
- 决定性环境在测试内固定:`TZ`、`LANG`、`FONTCONFIG_FILE`、`XDG_CACHE_HOME`;字体固定(随仓库一个 ttf,
  或测试只用系统字体)——否则金标准测试随机器变红,S8 的 chroot 会先炸
- fixtures:授权干净(CC0)、尺寸小、随仓库提交;程序化生成的图用固定种子
- 契约 v1 必须**显式列出 v1 的非目标**(每格独立文字、嵌套组、混合模式、>±45° 旋转、源 ICC 保留…),
  否则"将来不够用"这件事没人看得见

**S2**
- 建议**裁剪几何限定为多边形**(或明确定义曲线离散化容差):面积、零重叠、无空洞才都是解析可判定的;
  曲线会把这三条变成"容差内大概对"
- 模板生成器随仓库提交(bin,不是 `build.rs`),并测试:重新生成 → 与固化数据逐字节相同
- 不引入外部 SVG 解析器(AGENTS:依赖最少);路径就是模板数据里的命令表
- CLI 补两个面向调用方的子命令(S1 已冻结机器面,这两个只是加数据来源):
  `templates --json` 出模板名 / 格数 / 长宽比,`init --template <name> --out x.pixlay` 出可加载的默认工程——
  调用方(含 AI)不必读源码就能选模板、也不必手写 `.pixlay` JSON(不引 `schemars`)

**S3**
- **先裁定退化策略再写判据。** AGENTS 的待解决项(细长格需 6.7–7.6× 缩放)会改变 clamp 契约
  (clamp 结果必须能报告"旋转被限"),而契约在 S1 冻结 → 这条必须在 **S1 契约评审前**定,不能留到 S3。
- "覆盖整个格子"的 epsilon 给数值(建议归一化 1e-6,或 300dpi 下 ≤0.5px),写进测试常量
- 命中测试(见 S6.5)与 clamp 同属几何,可并到本步

**S4**
- 明确**缓冲阶梯**:哪几级是 16-bit、在什么分辨率上。全画布 16-bit RGBA 实测 1064 MB,10 张图不可能这么算。
  建议:解码 → 16-bit 线性 → **降采样到格内显示尺寸** → 调色 → 全局滤镜 → sRGB 8-bit → Cairo;
  峰值 = Σ(格内尺寸缓冲) + 输出表面 = O(输出像素)
- 解码给尺寸上限(否则 100MP 手机图直接进内存);并发度按内存预算设上限,不是 `nproc`
- 色彩决策写进契约:是否尊重源 ICC、输出 ICC 用 sRGB v2 还是 v4、CMYK JPEG、**源 alpha 怎么处理**
  (S1 的"带 alpha 的 PNG" fixture 目前没有对应的期望行为)
- "降采样目视无锯齿" → 与独立实现对比(ImageMagick `-filter Lanczos -resize`)的 RMSE 阈值,
  再加一个 zone-plate 检查混叠能量

**S5**
- "CJK 避头尾正确"是可判定的:构造行首禁则标点的文本,Pango 分行后用 `pango_layout_get_line*`
  断言没有一行以禁则字符开头;标点挤压断言相邻标点字距小于默认字距
- 字号必须用**归一化画布相对单位**,否则预览/导出 RMSE 判据立刻失败

**S6**
- "编码与元数据一趟完成"的实现含义:**cairo 只提供像素,编码器自己写元数据**。实测
  `cairo_surface_write_to_png` 在 A0 上只写 IHDR/bKGD/IDAT——**没有 pHYs、没有 iCCP**
  (`identify` 报 `Units: Undefined`),`set_fallback_resolution` 对位图后端无效。用 cairo 写 PNG 就必然丢 DPI。
- 每格式字段清单写进判据:PNG = pHYs + iCCP(+ sRGB chunk);JPEG = JFIF density 或 EXIF resolution + APP2 ICC;
  TIFF = XResolution/ResolutionUnit + ICCProfile
- 长边像素模式的取整规则、以及"此模式下 DPI 写什么"必须定义并测试(AGENTS 要求任何输出都带 DPI)
- 时间上限用相对值(≤ 3× 基线),基线见「实测基线」
- **产物落磁盘路径**:本机 `/tmp` 是 tmpfs(7.5 GB),A0 照片 PNG 342 MB、TIFF 476 MB,写 tmpfs 等于再吃一份内存

**S7**
- "三分钟走通"按人工判据给出(见「需要人工介入」):脚本化步骤清单 + 计时
- 补一条:大图导出时 UI 不冻结(导出在后台 + 进度反馈)——S0/S6 的操作是 6–7 s 级
- **i18n 机制(已定)**:`gettext`(crate `gettext-rs`,**不 pin 版本**——照「追新」由 S7 的 `cargo update` 定),
  域 `pixlay`,源语言英文,`.pot` + `po/POTFILES` 随仓库提交;依赖只进 `pixlay`(AGENTS 已禁
  core/imaging/render/cli 引 i18n);该依赖与其余新依赖一并在「依赖登记」里登记。
  *理由:GTK 与 libadwaita 自身的按钮文案走系统 gettext,`.desktop` 与 AppStream metainfo 的翻译(S8)用同一套
  `xgettext` / `msgfmt` 工具链——换成 fluent 类方案要自己接 metainfo 那半边。*
- 抽取命令用 **`xgettext --language=Rust`**:gettext-tools 1.0 的 Rust 后端实测能提
  `gettext` / `ngettext` / `pgettext`,并给 `ngettext` 的两条 msgid 打上 `#, rust-format` 标记;
  **别用 `--language=C` 绕**——能提出串,但丢掉 `rust-format`,`msgfmt --check-format` 就校验不了 `{}` 占位符
  (抽取与 `msgfmt` 的行为是 2026-09-20 在本机 `gettext-tools 1.0` 上实测的,不是记忆)
- `msgfmt --xml`(metainfo)与 `msgfmt --desktop`(desktop 文件)在 S8 与 `.po` 走同一条管线
- 判据补充:`LANG` 缺失 / `C` / 未知语言三种情况下 GUI 都出英文且能启动(缺翻译回退是 gettext 默认行为,
  一条冒烟即可);本步**不提交任何 `.po` 翻译**,抽取与回退先成立即可
- 硬编码串的判定边界:文案是否漏包装**不做机器判据**(提取器看不到漏掉的字符串),只做
  `po/POTFILES` 与 `crates/pixlay/src/**/*.rs` 的集合比对 + 人工目视

**S8**
- `check()` 里跑测试要分层:重测试(A0、HEIC、字体排版)标 `#[ignore]` 或 feature-gated,
  `check()` 只跑快层——干净 chroot 里没有字体缓存、没有 `$HOME`、没有显示器
- 补 `.pixlay` 的 MIME 注册(shared-mime-info xml + desktop 文件 + 图标);`depends` 清单要按解码后端确定
- 补 AppStream metainfo:文件名与 `<id>` **必须等于 app-id**(`org.yangtse.Pixlay.metainfo.xml`),
  `<url type="homepage">` 指向 `yangtse.org` 下的实际页面;PKGBUILD 的 `url=` 与 `pkgdesc` 同样用英文并指向该页面。
  缺 metainfo 的后果不是报错,而是软件中心里没名字、没截图、没图标
- i18n 落地:`.desktop` 与 AppStream metainfo 的多语言字段(S7 已定 gettext)与 `.po` 一起装;
  `check()` 里 `msgfmt --check` 必须离线可跑,且**没有任何语言包时界面仍是英文**
- `makepkg` 的 `check()` 必须离线可跑 → fixtures 全在仓库内,`cargo vendor` 不能漏测试依赖

### 四、度量口径(数字能互相比较的前提)

- 峰值内存 = `/proc/self/status` 的 `VmHWM`(或 `getrusage.ru_maxrss`),不用 RSS 采样
- 时间 = `CLOCK_MONOTONIC` 墙钟,合成与编码分开报
- 测试内容必须非平坦
- 产物写磁盘(`/var/tmp` 或 `$XDG_CACHE_HOME`),不写 `/tmp`
- 每个阈值常量在代码里注释**来源**(实测值 + 日期)——AGENTS 的"只留测试"才成立

### 五、开工前待决策

按"错了要返工多少"分三档。第三列是建议,**未反对即按建议锁定**。

#### A. 已落地(2026-09-20)

脚手架进仓库,`cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test` / `cargo build --release` 全绿。

- 构建档:基线一律 `--release`;`[profile.release]` 冻在 `lto = "thin"`、`codegen-units = 1`、`debug = 1`
- `[profile.dev.package."*"] opt-level = 2`:只作用于**外部依赖**(实测工作区成员不带 `-C opt-level`,仍是 0),
  目的是让 S4 的图像管线在 dev 下可用,同时不拖慢自身 crate 的迭代编译
- workspace:`edition = "2024"`、`resolver = "3"`、`rust-version = "1.98"`(跟 Arch 现装 rustc,
  见 AGENTS 的「版本策略:追新」而不是 edition 下限);`crates/*` 五个 crate 齐(见下)
- license:每 crate `license = "GPL-3.0-or-later"` + 根 `LICENSE`(SPDX 原文)+ `publish = false`
- 门禁:`cargo fmt --check` 与 `cargo clippy --workspace --all-targets -- -D warnings` 已进「验证入口」;
  lint 在根 `Cargo.toml` 的 `[workspace.lints]` 统一设(`unsafe_code = deny`、clippy `all` 全 deny、
  `dbg_macro` / `todo` / `unimplemented` deny —— 后两条同时机械封住"留 TODO 占位"这条路)
- CI:S0–S6 不做;S8 加 Arch 容器 job

脚手架现状(给下一个会话):

    Cargo.toml            工作区 + profile + lints;members = crates/*
    rustfmt.toml          edition/style_edition = 2024
    LICENSE               GPL-3.0-or-later(SPDX 原文,232 行)
    crates/pixlay-core    文档骨架,内容来自 S1/S2/S3/S6.5;禁止 gtk/cairo
    crates/pixlay-imaging 文档骨架,S4;暴露同步纯函数(线程归调用方)
    crates/pixlay-render  文档骨架,S1;唯一的 draw(doc, target)
    crates/pixlay-cli     bin 名为 `pixlay-render`(非 `pixlay-cli`),当前是以退出码 2 报"未实现"的占位;
                          禁止 gtk4
    crates/pixlay         lib 骨架,只钉住 `APP_ID`;GUI 与 `pixlay` bin 目标 S7 才加
                          (这样 S0–S6 的 `cargo test` 不必编译 gtk4-rs)

未做:`cargo vendor` 目前是空依赖,验不了这条判据;等 S1 引入首批依赖后再验。

#### B. 已确认(2026-09-20)——S1 契约冻结前必须遵守

| 条目 | 决定 |
|---|---|
| 画布预设集 | A4 / A3 / A0 各纵横 + 1:1 + 3:2 + 4:3 + 16:9 + 自定义;`Template` **声明所属长宽比**,模板矩阵按长宽比分族 |
| `.pixlay` 格式 | JSON(`serde_json`) |
| 工程版本策略 | 加 `docVersion`;读到更高版本**直接拒绝并报错**,不猜、不降级 |
| `draw` 的 target | `{ cairo ctx, scale, band }`——预览缩放与 A0 分块渲染都只是调用方参数,不改渲染器 |
| 错误类型策略 | core/render 用 `thiserror` typed error;`anyhow` 只出现在 `pixlay-cli` |
| imaging 线程模型 | 暴露**同步纯函数**,不带线程/通道;并发归调用方(GUI 自己经 channel 回主线程) |
| 格内源图 alpha | 合成到**不透明白**;导出永远不透明;预览与导出像素一致 |
| 文字层 v1 | 字号**归一化**;token 仅 `{date}` / `{filename}` / `{index}`,其余进「v1 非目标」 |
| `{date}` 口径 | 取 EXIF `DateTimeOriginal` **原样不换算**时区;缺失回退到工程里存的字符串;测试 pin `TZ` |
| 撤销粒度与快照 | 一次手势 = 一条命令(拖动结束才提交);快照存整份 `CollageDoc`,不做 diff |
| 配置存放 | `~/.config/pixlay/` 下的 serde 文件(与 `.pixlay` 同一套);**不用 GSettings** |
| 上限常数 | 画布 ≤ **200 MP**、DPI **72–600**、槽数 **2–10**;越界给明确错误,不 panic(A0@300dpi = 139.5 MP,留 43% 余量) |
| 细长格 clamp 退化 | 所需缩放 > **1.5×** 时**限制旋转角**;clamp 结果带「被限」标记给 UI |
| `.pixlay` 路径解析 | 相对工程文件;缺文件 = 明确报错 + 非零退出码;原子写(tmp + rename) |

#### C. S1 之后、S4 之前(仍是建议,未反对即按建议锁定)

| 问题 | 建议 |
|---|---|
| 源 ICC | v1 不读源 ICC,一律按 sRGB 解释,并在文档里写明这是已知限制(真做需要 lcms2 + 渲染意图定义) |
| 输出 ICC | 内嵌 sRGB IEC61966-2.1 profile 字节,不引 lcms2 |
| 解码后端 | 见下方「解码后端的两条路」;先实测再定,**这个决定 S8 的 `depends`** |
| 字体 | 生产不捆绑字体(Noto Sans CJK 太大);金标准文本测试用仓库内小号测试字体,系统字体渲染的检查标 `#[ignore]` |
| 依赖登记 | 每个新依赖在 AGENTS 登记(name / version / 为什么 / 体积);S1 首批一次性登记 |
| AUR 包名与版本 | 包名 `pixlay`;release tag `vX.Y.Z` 打到 GitHub,PKGBUILD `source=` 用 tag tarball |

**解码后端的两条路**(来源:`glycin-core 4.0.0` / `glycin 4.0.0` 源码实测,不是记忆):

- `glycin-core 4.0.0` 的 `COMPAT_VERSION = 2` → 它认 `/usr/share/glycin-loaders/2+/conf.d/`,
  与 Arch `glycin` 包已装的 `2+` loader **兼容**(不存在"crate 4 要 loader 4+"的问题)。
- 但 `glycin` facade 在 Linux 上**硬依赖 `glycin-external`**(`[target.'cfg(target_os = "linux")'.dependencies]`
  里非 optional),即必须走**沙箱 loader 进程**:需要 libseccomp、bwrap、系统 loader 包,
  以及 D-Bus 连接(loader 二进制要 `--dbus-fd`)。本机 `glycin-thumbnailer` 全格式失败那一测,
  是发行版二进制的问题,与这条 crate 路径不是一回事。
- **自包含方案**:直接依赖 `glycin-builtin`(=`glycin-core` + `builtin`)+ `builtin-image-rs` feature,
  loader 在进程内,不需要 bwrap / D-Bus / 发行版 loader 包。代价:
  1. `glycin-image-rs` 覆盖 PNG/JPEG/WebP/TIFF/GIF/AVIF 等,**不含 HEIC**;
  2. HEIC 在 Arch 是独立的 `glycin-heif` loader(走 libheif),自包含方案要另找路径。
- S4 第一步:这两条各跑一次真实解码(含 HEIC、含 EXIF Orientation=6),用数字选一条。

**已定**:`app-id = org.yangtse.Pixlay`(自有域名 `yangtse.org`);仓库 `YangtseSu/pixlay`(private);提交纪律与语言约定(英文)。

### 六、实测基线(2026-09-20,本机)

环境:cairo 1.18.4 · pixman 0.46.4 · gtk4 4.22.5 · libadwaita 1.9.4 · rustc 1.98.1 · 24 线程 ·
15 GB RAM · `/tmp` tmpfs 剩 7.5 GB · lcms2 2.19.1 · libheif 1.23.4 · libavif · libjxl ·
libtiff 4.7.2 · libjpeg-turbo 3.2.0 · glycin 2.1.5(loaders `2+`:heif / image-rs / jxl / svg)· bwrap 在。

A0 = 9933×14043(139.5 MP)`ARGB32` 表面,stride 39732:

| 项 | 结果 |
|---|---|
| 表面创建 | 0.1 ms,558 MB |
| 填白 | 43 ms |
| 2 格合成(多边形 clip + 仿射贴图 + 硬填充) | 244 ms |
| 81 次贴图(照片内容) | 699 ms |
| 旋转 CJK 文字(pangocairo,Noto Sans CJK) | 16 ms |
| `write_to_png`(平坦内容) | 1497 ms → 4.4 MB |
| `write_to_png`(照片内容) | 6907 ms → 342 MB |
| → JPEG q90 4:4:4(magick) | 5.5 s → 150 MB |
| → TIFF LZW(magick) | 4.8 s → 476 MB |
| 全画布 16-bit RGBA 中间缓冲 | 1064 MB 常驻,120 ms 触碰完 |
| 整程峰值 `VmHWM` | **543 MB**(含 558 MB 表面;带文字层的那次 569 MB) |

**接缝**(AGENTS 的"必须在 Cairo 上重测")已测:相邻格共用边上的混色像素数 / 缝长 ≈ **1.08**
(缝长为几何长度估计),在 A0 与 1/5 尺寸下**完全相同** → 混色宽度是 1 物理像素、与输出分辨率无关,
且无强渗色(强混色像素数 0)。**推论:缝可见的是预览(低分辨率),不是导出**——300dpi 下 1px ≈ 0.085 mm。
判据写成"混色像素数 ≤ 2 × 缝长",两种尺寸各测一次。

**glycin**:本机 `glycin-thumbnailer` 对 PNG / JPEG / HEIC / AVIF **全部**失败
(`Failed to load file/stream: Operation not supported`),带 session bus 与不带都一样;loader 二进制与 bwrap 都在。
原因未定位,但足以说明"glycin 非 Flatpak 零配置可用"**尚未成立** → S4 第一条必须是先证明它,
且 CLI/测试要有一条不依赖 glycin 的路。

#### S0 复测(2026-09-20,spike 进仓库之后,`--release`,同一台机器)

上面那次是一次性探针(未进仓库),这次是仓库内 `a0-spike` 的正式判据。**内容模型不同**:
照片按**格内显示尺寸 1:1** 生成(每像素带颗粒),而不是小图重复贴——这是 S4 缓冲阶梯
(`解码 → 16-bit 线性 → 降采样到格内显示尺寸 → 调色 → 合成`)实际会拿到的输入形状。

| 配置 | 合成 ms | PNG ms / MB | JPEG ms / MB | `VmHWM` 合成 / 整程 MB |
|---|---|---|---|---|
| flat 2 格 | 189 | — | — | 941 / 941 |
| flat 10 格 | 550 | — | — | 941 / 941 |
| detail 2 格 | 185 | 34002 / 120.0 | 3315 / 38.3 | 941 / 1340 |
| detail 10 格 | 551 | 27773 / 125.6 | 3240 / 40.3 | 941 / 1340 |

内存构成:输出表面 **558 MB** + 照片缓冲 **402 MB**(两档格数巧合相同:2 格时是两个大格,
10 格时是一个大格加九个小格)= 960 MB,与实测 941 MB 一致。**10 格不额外吃内存**——峰值由
输出表面加"格内显示尺寸的照片总和"决定,与格数无关,这是 S4 该照抄的结论。

与上面一次性探针的差异,两条都要记:

1. **PNG 慢得多:34 s / 120 MB vs 6.9 s / 342 MB。** 差别在内容:逐像素熵高的 A0 内容让 zlib
   真干满 558 MB 的活(4 Mpx/s),而探针那次的"照片内容"可压缩性高得多(输出更大反而更快)。
   `cairo_surface_write_to_png` 是**单线程 zlib**,S6 本来就因为元数据放弃它,现在多一条性能理由。
2. **峰值不再是 543 MB 而接近 1 GB**,因为这次把照片缓冲算进去了(见上)。2.5 GB 预算仍有 1.6 GB 余量。

`--preview-px` 出来的 1400×1979 预览(两张)已目视:不规则 L 形 clip 正确、旋转格被 clip 住、
未覆盖处露白、CJK 字形完整、缝无渗色。

---

## 完成后

S0–S8 全部勾选后,本文件可以删。届时该有的约束已在 `AGENTS.md`,该有的规格已在测试里。
