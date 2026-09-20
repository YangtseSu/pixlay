# S1 contract v1

冻结于 2026-09-20。这份文件是**契约评审**的读本:评审过的每个形状、每条限制、每个非目标都在这里。
实现是权威,本文件是它的导读;两者不一致时以测试为准(测试在
`crates/pixlay-core/tests/`、`crates/pixlay-render/tests/`、`crates/pixlay-cli/tests/`)。

按 `docs/STEPS.md` 的切分原则,契约必须**在 S1 冻结**,因为后面八步全部盖在它上面。

---

## 一、`CollageDoc`:唯一的序列化形状

```jsonc
{
  "docVersion": 1,                 // 格式版本;读到更高的版本直接拒绝,不猜不降级
  "canvas": { "widthMm": 297.0, "heightMm": 210.0 },
  "template": {                    // 几何是数据,不是引用:库里改了模板也不动已存工程
    "name": "mosaic-8-s14",
    "version": 1,                  // 模板几何版本
    "aspect": 1.3333333333333333,  // 长宽比(宽/高),模板矩阵按它分族
    "slots": [
      { "outline": [[0,0],[0.375,0],[0.375,0.375],[0,0.375]], "area": 0.140625 }
    ]
  },
  "cells": [                        // 每槽一格,顺序即 slot index
    { "source": "photos/a.jpg", "crop": { "zoom": 1.0, "offset": [0,0], "rotationDeg": 0 } }
  ],
  "text": [                         // 画布级文字层(S5 渲染)
    { "content": "{date} #{index}", "mode": {"kind":"free","position":[0.5,0.9],"anchor":"bottomCenter"},
      "sizeRel": 0.02, "rotationDeg": 6, "color": {"r":0,"g":0,"b":0,"a":255}, "sourceSlot": 0 }
  ],
  "textFallback": { "date": "2026-09-20" }
}
```

**版本策略**(S1 评审裁定,2026-09-20:**允许破坏,但不写迁移**)。

- **加字段不抬版本**:新字段一律带 `serde` 默认值(或 `Option`),旧工程照常读得开。
  这条是纪律:每加一个字段都问一遍「它能不能有默认值」,能就别抬版本。
- **改含义/删字段才抬 `DOC_VERSION`**:此时旧工程**直接拒绝**,报错要说明能怎么办
  (`document version N predates the current format M ... rebuild the project with this version`)。
- **不写迁移代码**。代价是明知的:旧工程报废、用户重建。收益是格式里不会有废弃字段,
  也不会有「迁移有 bug → 用户数据在这条路上丢」这条路径。
- 读到**更高**版本也拒绝(`rebuild` 不适用,报错只说版本太新)。
- `DOC_VERSION_MIN` 保留为常量而不删,是为了让「本 build 读哪个版本区间」在一处可读;
  现在恒等于 `DOC_VERSION`。

约定:

- **未知字段一律拒绝**(`deny_unknown_fields`)。拼错一个键不该静默变成默认值。
- **字段名 camelCase**;点写成两元数组 `[x, y]`(`Point`)——`.pixlay` 是给人看的。
- 所有坐标/尺寸/字号**归一化到 `[0,1]`**;绝对像素只在 `CanvasSpec::pixel_size(dpi)` 之后出现。
  归一化字号是为了「大字体」之类的 UI 缩放不得改变导出像素(见 `AGENTS.md` 硬约束)。
- **空槽 = `source: null`**,该格渲成白色。`source` 是相对工程文件的路径;缺文件 → 明确报错,
  不是跳过。
- `crop.zoom` 是**绝对 zoom**(显示宽度 / 格子宽度),不是「相对铺满的倍数」:
  换照片时基准不变,取景不跳焦。
- `rotationDeg` 上限 ±45°,`crop.offset` 各分量 |offset| ≤ 1(超出后任何 clamp 都救不回覆盖)。
- **约定方向**:`rotationDeg` 与文字层的 `rotationDeg` 都是**屏幕上顺时针**为正(画布 y 朝下,
  cairo 在该空间里的 `rotate` 即顺时针,渲染器原样传递)。
- **文字层顺序**:`text` 按数组顺序绘制,后画的盖住先画的,全部盖在格子之上。
- **平铺相位**:瓦片网格从画布原点 `(0,0)` 起算,每个瓦片绕自己的锚点旋转;没有逐瓦片变化。

## 二、限制常数(全部有明确错误,不 panic)

| 项 | 值 | 来源 |
|---|---|---|
| `docVersion` | 恰好 `DOC_VERSION`(现 1);更高拒绝、更低也拒绝 | 见「版本策略」 |
| 槽数 | 2..=10 | `AGENTS.md` |
| DPI | 72..=600 | `AGENTS.md` |
| 画布像素 | ≤ 200 MP | A0@300dpi = 139.5 MP,留 43% 余量 |
| 画布边长 | ≤ 2000 mm | 大于任何输出设备 |
| 取景旋转 | ±45°(顺时针为正,画布 y 朝下) | `AGENTS.md` |
| 取景 zoom | `0 < zoom ≤ 1000` | 上界是必需的:zoom 决定解码位图的尺寸,无上界会溢出。S4 的解码器**另按内存预算**设上限,两层各管各的 |
| 画布 vs 模板长宽比 | 差 ≤ 1e-6,否则硬错误 | 两者各自独立标注,归一化坐标自身不带长宽比;GUI 的模板选择器按长宽比分族,只列出匹配的 |
| 文本字号 | 0 < `sizeRel` ≤ 1.0(画布高占比) | — |
| 平铺步距 | 两个分量都 > 0 | 步距 0 或负值会让平铺无限循环 |
| clamp 退化阈值 | 所需缩放 > 1.5× 时**限制旋转角** | `docs/STEPS.md` §五.B |

## 三、模板

- `Slot::outline` 是**闭合多边形**(末点连回首点),顶点数 ≥ 3、有限、落在 `[0,1]`、面积 > 0。
- `Slot::area` 是声明面积,与 outline 的实际面积交叉校验(容差 1e-6)。两者不许漂移。
- S2 的完整不变量(两两零重叠、并集无内部空洞、cut 类面积和恰为 1.0)在 S2 的测试里;
  v1 的 `Template` 形状已经装得下它们(outline 是命令表,不需要 SVG 解析器)。
- **几何版本与文档版本分离**:`template.version` 随模板家族走,`docVersion` 随格式走。
- **模板库在 `pixlay-core`**(`pixlay_core::templates`):模板是文档数据,不是 CLI 的资源——
  GUI 要靠它出模板选择器,CLI 要靠它出 smoke 渲染。S1 的 `mosaic-8-s14` 是手写的 cut 几何
  (一格异形、坐标为 1/8 的整数倍、声明面积和**恰好** 1.0),S2 换成生成器但**名字与版本不变**;
  其不变量(零重叠、无空洞、面积和、确定性)已由 `crates/pixlay-core/tests/templates.rs` 先钉住。
- **画布长宽比必须与模板声明一致**(容差 1e-6):几何是归一化的,声明一个不同比例的画布会把模板
  静默拉伸,而这条错在哪一层都看不出来。

## 四、渲染:`draw(doc, images, target)`

唯一的渲染实现。预览与导出是**同一个函数**,差别只有 `scale`(与 `band`)。

```text
draw(&CollageDoc, &Images, &Target) -> Result<(), RenderError>

Target { ctx, scale, canvas_px, band }
Band  { index, count }            // 水平分块:A0 可只渲染一条
Bitmap                            // ARgb32 预乘,已是显示尺寸
Images                            // 槽位 → Bitmap;缺 = 该格留白
```

- **Cairo 只 blit 与 clip**:进来的位图已经解码、降采样、旋转、调色完毕(`pixlay-imaging`,S4)。
- 合成到**不透明白底**;输出永远不透明。
- **分块渲染**:`Band::out_rows()` 在**输出像素**上划分(`first = total * index / count`),
  所以任意 `scale` 下各块的尺寸和恰好等于整图。此前按画布行划分、每块各自取整,
  在 72dpi/scale=0.3 下三块合计 759 行而整图 758 行——`round` 不可加,这条只能这样修。
  实测整图与三块拼接的 RMSE 0.033(scale 1.0;见下),scale 0.1/0.3/0.5 也测。
- 文字层在 v1 里**会被 `draw` 拒绝**(`TextLayersUnsupported`)——宁可报错,也不导出丢字的东西。
  S5 把它接上,契约不变。
- `render_surface` / `render_rgb8` 只是分配表面 + 调 `draw` 的薄壳;`rgb8` 把 ARgb32 预乘统一
  合成到白底,给出编码器要的直通 RGB。

## 五、CLI:机器操作面

```text
pixlay-render render --project <file.pixlay> --dpi <n> --out <file>
pixlay-render render --template <name> --dpi <n> --out <file>   # 无工程无照片
pixlay-render probe  --project <file.pixlay>
```

| 项 | 契约 |
|---|---|
| stdout | **只有**机器可读结果(排序的 `key = value`,或 `--json` 一个对象)。诊断全在 stderr |
| 稳定性 | 同输入同输出;结果里没有时间戳、没有绝对路径。`--stats` 的 `ms`/`encode_ms`/`peak_rss_mb` 是**唯一**例外(它就是测量值) |
| locale | `LANG` / `LC_ALL` / `LANGUAGE` 任意取值下 stdout 与 stderr **逐字节相同**(含错误分支) |
| 交互 | 不读 stdin、不等提示、无 TTY 照常;`--help` 覆盖全部 flag 与退出码 |
| 退出码 | 0 成功 / 1 用法错 / 2 工程、解码或渲染失败 / 2 probe 判定不通过 |
| 用法错与「没能产出结果」 | stdout 保持为空;stderr 点名出错的路径(或缺哪个 flag) |
| probe 判定不通过 | **不是「没能产出结果」**:数字就是结果,所以 stdout 照常出全部数字,`status = failed`、`passed = false`,stderr 出一行综述,退出码 2 |
| probe 的下界 | `occupied = 0`(全是空槽)时判 **failed**:探针的每个问题都是关于某个格子的,没格子就没结论。此前它会「空过」(status=ok、exit 0) |
| 输出格式 | 由 `--out` 扩展名决定:`.png` / `.jpg` / `.jpeg`,其余是用法错 |
| `--preview-px n` | 长边 n 像素;同一个 `draw`,只改 `scale` |
| `--stats` | 追加 `{ms, encode_ms, peak_rss_mb, icc}`;口径见下 |
| `probe` | 采样并输出数字(格内照片色、格外白底、共用边混色、三色凸组合残差),判定不通过时退出码 2 |

度量口径(`AGENTS.md`):峰值 = `/proc/self/status` 的 `VmHWM`;时间 = 墙钟,合成与编码分开报。

`probe` 的阈值常量(代码内注释了来源):

| 常量 | 值 | 来源 |
|---|---|---|
| 格内采样颜色 | **精确相等**(无容差) | 采样点取「距边界最远点」,远离抗锯齿边界 |
| 缝混色上限 | ≤ 2 px/行,最宽 ≤ 2 px | S0 实测 1.08 px/行、最宽 1 px |
| 三色凸组合残差上限 | 3.0/255 | S0 实测 0.20/255;300dpi 八格实测最差 0.63 |

`probe` 用**平坦**内容(每格一色):只有色块平了,缝上的混色才能与内容区分开。几何格内采样点取
「距边界最远的点」(网格粗搜 + 逐级收敛),所以 L 形格的包围盒中心不会被误用。

## 六、v1 非目标(必须显式列出,否则"将来不够用"没人看得见)

- 每格独立文字层(文字**只有画布级**;平铺水印是它的一种模式,不是第二套机制)
- 嵌套组 / 图层树 / 混合模式
- 超过 ±45° 的取景旋转;**旋转只裁边,不扩画布**
- 保留源 ICC(v1 一律按 sRGB 解释)、CMYK JPEG、每格独立色彩空间
- 曲线 / 色阶 / 遮蔽 / 画笔;全在 `AGENTS.md` 的「不做」
- 多页 / 多画布工程
- `.pixlay` 的**版本迁移**(不写;更高拒绝、更低也拒绝,见「版本策略」)
- **写 `.pixlay`**(S1 只读;原子写 tmp+rename 与写盘路径的等价性由 S6.5 负责)
- MCP server、REPL / watch、自然语言参数、从配置文件读默认值

## 七、待实现但形状已冻结

| 项 | 落在 | 形状 |
|---|---|---|
| clamp 数学 | S3 | `CropTransform` + `CropFit { transform, rotation_limited }`(类型已在 `pixlay-core`) |
| 模板生成器 | S2 | `Template` 的 outline 命令表;`templates --json` / `init` 子命令 |
| 命令历史 / 命中测试 / 工程写 | S6.5 | 未进 S1 契约;`CollageDoc` 是它们的状态载体 |
| 文字渲染 | S5 | `TextLayer` 已在契约里;`draw` 现在拒绝它 |
| 编码与元数据 | S6 | 现在用 `image` crate 顶替;S6 换成一趟写像素 + 色彩采样 + ICC + DPI |

## 八、实测(2026-09-20,本机)

| 项 | 值 |
|---|---|
| 金标准图 RMSE | **0.0**(同 build 确定);1 px 几何错的等效 RMSE 12 |
| 预览 vs 导出(2N 降采样) | RMSE **2.32**(阈值 6,`AGENTS.md` 的 A0 实测 2.62) |
| 分块拼接 vs 整图 | RMSE **0.033**,最大像素差 2/255,311 / 463080 字节不同 |
| 缝混色 `probe` | ≤ 2 px/行(阈值),`foreign = 0` |

测试里每个阈值常量都注明了这条来源,所以数字变了能被发现。
