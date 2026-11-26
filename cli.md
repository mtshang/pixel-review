# CLI 接口总览

| 任务 | 调用形式 | 主要结果 |
|---|---|---|
| 测量整个 PNG 的非透明内容 | `bounds --source FILE` | 画布宽高、非透明边界及宽高、四边留白 |
| 读取像素矩阵 | `matrix --source FILE [--point X,Y]... [--region X,Y,W,H]... [--out NEW.json]` | 整图、多个单点或多个矩形的 RGBA 色码 |
| 生成整数倍预览 PNG | `zoom --source FILE`，可选 `--target-long-edge PX` 或 `--scale N` | 独立的预览文件和坐标映射所需参数 |
| 查询原图与预览的一组位置 | `pick --source FILE --scale N` 加一个或多个选点参数 | 两套坐标、两个实际颜色、覆盖的预览块 |
| 校验预览 | `compare --source FILE --scale N` | 对比结论、差异数与样本 |
| 修改源像素并生成预览 | `edit --source FILE --scale N` 加一个或多个设色参数 | 编辑版原图、预览、修改前后颜色 |
| 批量绘制并生成预览 | `edit --source FILE --scale N --patch FILE` | 编辑版、预览及写入统计 |
| 复制任意点和矩形的并集 | `copy --source FILE (--point X,Y 或 --rect X,Y,W,H)… --out NEW.json` | 可重复使用或跨图粘贴的剪贴文件 |
| 移动或粘贴整组选区 | `edit --source FILE --scale N` 加 `--move-by` 与选区，或 `--paste` 与 `--at` | 候选 PNG、预览及落点坐标 |
| 批量修改导出的颜色表 | `edit-matrix --source MATRIX.json --patch FILE --out NEW.json` | 保留原图坐标的新版矩阵文件 |
| 导出许可 | `licenses --out NEW.zip` | 新建的第三方声明归档 |
| 对照方案、修改前后与游戏场景 | `review-sheet --source A.png [--source B.png] [--before OLD.png] --out NEW.png` | 编号评审图与每列来源、角色和各面板的矩形 |
| 保存动画配置并观察 | `animation-config --frame A.png --frame B.png --out CFG.json`，随后 `animation` 或 `onion` 传 `--config CFG.json` | 有序帧、循环 APNG、编号横排图或指定帧叠看 |
| 调整画布边界 | `resize --source FILE --out NEW.png` 或 `canvas resize --canvas NAME` 加边距或裁框 | 新尺寸、坐标平移量和基线位置 |

以下示例在 skill 根目录的 PowerShell 中运行。路径可使用绝对路径，包含空格时加引号。调用时以 UTF-8 读取标准输出，每次调用输出一个 JSON 对象。帮助命令为 `.\scripts\pixel-review.exe --help`。可执行程序面向 Windows x64。

~~~powershell
$px = '.\scripts\pixel-review.exe'
$src = '.\examples\cat.png'
& $px zoom --source $src --scale 8 --out-dir '.\work' --out-name 'cat-large.png'
& $px pick --source $src --scale 8 --preview '.\work\cat-large.png' --pick-preview '35,42'
& $px compare --source $src --scale 8 --preview '.\work\cat-large.png'
~~~

## 输入与定位

### 方案评审图

每列对应一张原尺寸静态 PNG。各版本先平移到共同画布，空白补透明。第一行显示全身，第二行看同一局部，第三行在场景内观察对比与遮挡。显示与场景默认同倍率。原图坐标加该列 offset 即为共同画布坐标。

| 参数 | 作用 |
|---|---|
| `--source PNG` | 至少一次，重复时按参数顺序从左到右排列候选。单张素材也可使用。 |
| `--before PNG` | 可选，将修改前原图固定放在第一列。 |
| `--display-scale N` | 第一行的正整数窗口倍率。默认 1；有项目约定时显式传入。 |
| `--region X,Y,W,H`、`--detail-scale N` | 第二行在共同画布上的裁框与正整数倍率，省略裁框则看全图。默认按细节长边最接近 320 像素选整数倍率，最小 1。常规 zoom 仍默认以 1600 为目标。 |
| `--background RRGGBB` | 前两行的共同底色，默认 80858C。 |
| `--anchor X,Y` | 原图中的水平参考像素和脚底行。从 0 起算。给一次表示所有版本共用该点，或按列顺序各给一个（before 在首列）。缺省时同尺寸保留原始坐标，不同尺寸取各图最低可见行、该行最左和最右可见像素的中点向下取整。全透明图使用画布水平中心与底行。对跳跃、拖地衣摆和投影宜指定锚点。 |
| `--scene PNG`、`--foreground PNG` | 按底图、候选、前景顺序合成。前景须与底图同尺寸，可重复传入，最后一个在最上层。PNG 模式要求共同画布完整落在场景内。 |
| `--scene 工程目录 --replace-layer 层名` | 读取已保存的分层工程，临时替换指定层。层序、可见性、各层位置、画布边缘裁切沿用工程。替换层自身隐藏时场景中也隐藏候选。默认将第 1 列原图左上角放在被替换层原来的落点，所有版本共用由此算出的定位。 |
| `--at X,Y`、`--scene-scale N` | at 用场景原图坐标指定共同画布左上角，PNG 模式省略时居中。场景先在原尺寸合成再最近邻放大，默认采用 display-scale；需要单独观察时传正整数 scene-scale。 |
| `--out NEW.png` | 创建新的组合 PNG。preview 返回路径和宽高。columns 含 number、role（before 或 candidate）、path 和各面板 display、detail、可选 scene 的矩形，坐标系是输出整图。回执另含 native_size、detail_region、实际倍率、background、scene 和 at，便于下一轮沿用。native_size 为共同画布宽高，common_anchor 是对齐点。各列另有 source_size、anchor 和 offset。scene_scale 给出实际场景倍率，foregrounds 和 replace_layer 记录遮挡来源。 |

### 地砖与动画预览

三个命令均输出到新的文件路径，支持 `--scale N` 或 `--target-long-edge PX`，默认目标长边 1600。地砖倍率按整个铺设结果计算，动画倍率按每帧的完整画布计算。输入均为静态 PNG；工程可先用 canvas export 输出合成或单层。

| 命令 | 参数与坐标 |
|---|---|
| `tile-preview --source TILE --out NEW.png` | 默认横纵各 3 块，可用 `--repeat 列数,行数`。图集内取一块用 `--region X,Y,W,H`，裁取后再重复。 |
| `tile-preview --layout FILE.json --out NEW.png` | 文件以 UTF-8 编码。tiles 将单字符映射到 source（相对布局文件或绝对路径）和可选 region；rows 是非空、等宽字符串数组，按行从左往右拼。所有素材取景后宽高一致。空白格可映射到透明砖。 |
| 两种 tile-preview 的可选参数 | `--overlay ROLE.png --at X,Y` 将角色左上角放在拼接后的原尺寸坐标，按 RGBA 透明度合成。角色需完全位于铺设区域内。结果含 tile_size、grid、native_size（均为宽/列在前）、scale 和 preview。 |
| `animation --frame A.png --frame B.png --out NEW.apng` | 至少两帧、宽高完全一致，顺序由重复的 --frame 决定，允许重复同一张图。`--fps N` 接受 1…65535 的整数，默认 8；文件每帧时长 1/N 秒，无限循环，播放器实际速度受其实现约束。也可用 .png 扩展名。 |
| `onion --frame A.png --frame B.png --index N --out NEW.png` | 输出第 N 帧的静态叠看，N 从 1 起。前一帧为青色，后一帧为洋红，按该顺序叠在当前帧上。只叠与当前 RGBA 不同且相邻帧 alpha 大于零的位置。`--opacity 0..100` 默认 40%，乘以邻帧原透明度。端点只用存在的一侧；检查循环接点时可显式传末帧、首帧、第二帧并选 index 2。 |
| 两种帧预览的可选参数 | `--baseline Y` 在零起算原图第 Y 行下沿加一条金色虚线（预览图上 1 像素厚）。frame_count 是输入帧数。animation 的 frames 数组报告每帧，onion 仅报告当前帧，另给 previous、next 路径。bounds 来自绘制参考线前的源像素，bottom_y 是最后一个可见行，bottom_delta=bottom_y−Y，正值在参考线下方。全透明帧的 bounds、bottom_y、bottom_delta 为 null。 |

布局文件的最小例子为 `{"tiles":{"A":{"source":"atlas.png","region":[0,0,20,20]},"B":{"source":"atlas.png","region":[20,0,20,20]}},"rows":["AB","BA"]}`。[完整钟楼布局](examples/review/tower-layout.json)

**动画配置。** `animation-config --frame A.png --frame B.png [--fps N] [--baseline Y] --out NEW.json` 写出 UTF-8 JSON。格式示例为 `{"format":"pixel-review.animation/1","frames":["a.png","b.png"],"fps":6,"baseline":37}`。frames 是有序的静态 PNG 路径数组，至少两项，允许重复。相对路径以配置文件目录为起点，可以带子目录或 `..`。写出时优先采用相对路径，跨驱动器时采用绝对路径。fps 可省略，默认 8。baseline 缺省或 null 表示无参考线。配置保存时会检查各帧宽高和基线范围。[随包的四帧配置](examples/review/walk-right.json)

`animation`、`onion` 和 `animation-config` 均支持用 `--config CFG.json` 代替所有 `--frame`。`--baseline Y` 或 `--no-baseline` 临时覆盖配置，animation 和 animation-config 也可传 `--fps N`。animation-config 可将覆盖后的配置另存为新文件。读取配置时回执的 config 是其绝对路径。

**横向帧序图。** animation 默认同时写出 `名称.apng` 与 `名称.frames.png`。用 `--strip-out NEW.png` 指定横图路径，或用 `--no-strip` 仅生成动画。两个输出都须为新路径。strip 含 path、width、height、scale，关闭时为 null。帧号从 1 开始，印在独立标题栏内。frames[].strip_region 给出该帧画面在横图内的 x、y、width、height，坐标从 0 开始。固定倍率时两份预览采用同一倍率。自动模式下，播放按单帧、横图按排成一行的全部帧内容分别选整数倍率，编号和间隔另占空间。静态入口读 strip.path；实际播放请在浏览器中打开 preview.path。

### 画布扩边与裁切

| 调用或参数 | 含义 |
|---|---|
| `resize --source SRC.png --out NEW.png` | 对磁盘 PNG 应用下列边距或裁框，输出 RGBA8，空出的区域为透明黑。 |
| `canvas resize --canvas A` | 在内存中调整整个画布，一次操作可由 undo/redo 撤销或重做。普通 PNG 单层底图随画布改变大小。分层工程保留各层的本地像素，统一调整层在画布上的位置，包括隐藏层和画布外的层。 |
| `--left N`、`--right N`、`--top N`、`--bottom N` | 至少给一边，省略的边为 0。正数向外加 N 格，负数向内裁去相应格数，可混合使用。结果宽高须大于零。 |
| `--crop X,Y,W,H` | 以上一画布的零起算左上角及宽高裁切，框须在原画布内。与四个边距参数互斥。 |
| `--pad-layer NAME` | canvas resize 可重复指定。为所选图层在每个扩出的方向追加同样格数的透明像素，让新区域可在该层直接落笔。裁入的部分仍保留在工程图层中。 |
| `--baseline Y` | 给出操作前的有效脚底行。resize 对象含 old_size、new_size（宽、高）、origin_shift（dx、dy）、old_baseline、new_baseline 和 baseline_visible。缺省时三个基线字段为 null。 |

像素映射为 `(x+dx, y+dy)`。例如向左扩 4、顶部裁 2 后，旧 (3,37) 对应新 (7,35)。只改左右或底边可维持同一脚底行号。分层工程导出合成 PNG 时只显示画布范围，导出单层时得到完整图层。手动 save 保存工程尺寸和层位置。resize 的基线用于本次换算，动画配置由调用者据此更新。

### 空白画布、撤销和共享调色板

| 调用 | 输入与结果 |
|---|---|
| `create --size W,H --out NEW.png` | 正整数宽、高，创建 RGBA 全为零的 PNG，新路径的父目录可自动创建。 |
| `canvas create --canvas NAME --size W,H --out NEW.png` | 同时写出透明底图，开启带 Base 层的画布，初始 dirty=false。 |
| `canvas undo/redo --canvas NAME` | undo 和 redo 分别为独立命令，每次一步。响应中 undo_steps、redo_steps 为剩余步数。最近 32 步；修改、粘贴、换色、图层增删、层序、可见性、位置、活动层和工程色表都可回退。查询、预览、保存、失败和无净变化的像素操作不占步。保存点决定 dirty，关闭画布清空历史。 |
| `canvas palette-use --canvas NAME --palette FILE` | 载入共享色表快照。保留色表用 save --project NEW_DIR；已有工程直接 save。色表文件更新后可再次载入。 |
| `edit / edit-matrix / canvas edit` 加 `--patch FILE --palette PAL.json` | 以指定色表解析 rows。canvas edit 省略该参数时取工程色表，补丁内同名符号优先。 |
| `palette-check --palette PAL.json --source A.png [--source B.png] [--allow COLOR]…` | 报告每张图的 outside_pixels、outside_colors，以及至多 64 种表外色的 rgba、count、first 坐标。透明度为零的像素跳过，其余按 RGBA 精确匹配。成功生成报告时退出码为 0，合规性读 ok。 |
| `recolor --source A.png [--source B.png] --map OLD=NEW [--map OLD=NEW]… --out-dir NEW_DIR [--palette PAL.json]` | 按替换前的 RGBA 同时映射，可交换两色。输出目录须为新路径，输入文件名须彼此不同。所有输出准备完成后发布整批，保留源文件。 |
| `canvas palette-check / canvas recolor` | 分别为两个子命令，均须传 --canvas NAME。默认逐层处理，含隐藏层；--layer NAME 可限定单层。沿用工程色表或传 --palette FILE 临时指定。check 可重复 --allow；recolor 可重复 --map，一次跨层换色占一个撤销步。 |

色表文件示例为 `{"palette":{"H":"#BBCDEE","E":"#202536","_":"transparent"}}`。每个键是一个字符，点号保留给跳过格。--map 和 --allow 的 COLOR 可写六位 RGB、八位 RGBA、transparent 或共享表中的 @符号（如 @H）。在 PowerShell 中将映射整体加单引号。换色只改变像素；需要同步改用色规范时载入相应版本的色表。

### 分层工程

一张常驻画布管理一组有序 RGBA8 图层。下表的每条命令都需要 `canvas 动作 --canvas NAME`。用 `canvas open --canvas NAME --source PNG或工程目录` 打开。PNG 初始只有 Base。工程目录含 project.json 和每个图层自己的 PNG，移动或复制工程时携带整个目录。

| 动作 | 参数 | 行为 |
|---|---|---|
| status | 无 | storage 为 png 或 project；layers 从底到顶列出 name、at、visible、width、height，active_layer 是默认写入目标。 |
| layer-add | `--layer NAME [--source PNG 或 --size W,H] [--at X,Y]` | 将素材按原尺寸导入，或新建透明层。省略尺寸取画布大小，默认位置 (0,0)。顶层追加并选中，名称在工程内唯一。 |
| layer-select、layer-remove | `--layer NAME` | 切换活动层或移除一层。移除活动层后选中下方邻层；移除底层时选中新的底层。至少保留一层。 |
| layer-set | `--layer NAME` 加 `--at X,Y`、`--visible true/false`、`--index N` 中的一项或多项 | at 是图层左上角在画布上的绝对位置，可为负数。index 是调整后的位置，从底层 0 开始。超出画布的部分仍在图层里。 |
| edit | `[--layer NAME] --patch FILE` | 对指定层或活动层使用已有的批量写入格式；传实际文件路径。出错整批回滚。 |
| copy、paste | 已有参数，可加 `--layer NAME` | 从指定层复制或写入；默认活动层。选区和落点均用图层内坐标。 |
| preview | 已有参数，可加 `--layer NAME` | 省略 layer 时观察可见层合成结果，region 为画布坐标。指定 layer 时预览单层原生视野，region 为层内坐标，隐藏层也可查看。 |
| export | `--out NEW.png [--layer NAME]` | 以原尺寸导出可见层合成图，或导出指定层的完整原始像素。单层包含画布外的像素及透明格。导出不切换保存目标，不清除 dirty。 |
| save | `--project NEW_DIR` 或无参数 | 首次保存分层内容到新目录，随后直接 save 更新工程。再次指定全新目录可另存。多层内容展平用 export。 |

合成从下往上按正常透明度叠加。层内坐标加上 at 得到画布坐标。图层隐藏仅影响合成，单层显示、编辑和导出均可按名称进行。普通工程的 format 为 `pixel-review.project/1`，含工程色表的清单为 `pixel-review.project/2` 并保存 palette 快照，size 为画布宽高，active 为活动层，layers 数组按绘制顺序保存 name、at、visible 和相对 PNG 文件名。通过名称操作图层，保存后像素有变的层可能换文件名。写入时先准备 PNG，最后替换清单；重新排序或移层可复用原 PNG。若另一会话已更新磁盘工程，另存新目录可保留两个版本。

多次粘贴到同一张图或需要手动保存时，用下面的常驻画布接口。

### 常驻画布

所有命令均从 `canvas 动作 --canvas NAME` 开始。每个名称对应一个本机 Windows 子进程；名称区分大小写，支持中文。命令相对路径以发起调用的工作目录解析。每次成功响应含 command、canvas、pid、source（当前保存路径与尺寸）和 dirty（有待保存的编辑）。命令失败沿用退出码 2 和 JSON error。

| 动作 | 其他参数 | 效果 |
|---|---|---|
| open | `--source FILE` | 读取 PNG 并等待画布就绪，同名画布已打开时报错。 |
| status | 无 | 查询当前画布。 |
| copy | `--clip NEW.json` 和至少一个 `--point X,Y` 或 `--rect X,Y,W,H` | 从内存快照复制选区并集，剪贴文件父目录须存在，文件须为新路径。与单次 copy/edit 的剪贴文件通用。 |
| paste | `--clip FILE --at X,Y`，可重复 --at，可加 `--skip-transparent` | 每个落点都是选区包围框左上角。先校验全部落点，再按提交顺序写入。默认复制原始 RGBA。跳过透明时只略过 alpha=0 的格子，半透明颜色原值覆盖。 |
| preview | `--out NEW.png`，可选 `--region X,Y,W,H` 和 `--target-long-edge PX` 或 `--scale N` | 输出当前内存视图，可自动建目录。默认整图、目标长边 1600。返回 preview、region、scale。 |
| save | 可选 `--out NEW.png` | 普通 PNG 无参数时写回当前路径，带 --out 时另存并切换保存目标。分层工程使用上节的 save --project 或 save。通过临时文件提交，成功后 dirty=false。磁盘原文件内容发生变化时，写回会报错，可另存新路径。 |
| close | 可选 `--discard` | 退出画布进程。检测到待保存编辑时拒绝普通关闭；显式 discard 放弃编辑。 |

每个画布串行处理请求，不同画布可以同时工作。一次 paste 成功返回后再提交依赖它的命令。客户端断开后若结果不明，检查内存预览及 status。进程异常退出会丢失上次保存后的改动；剪贴文件独立保存在磁盘。预览和另存均使用新文件名。

| 参数 | 适用命令 | 约定 |
|---|---|---|
| `--source FILE` | 所有图像命令 | 静态 PNG。支持索引色、灰度、灰度加透明度、RGB、RGBA、tRNS 和 Adam7。接受每通道至多 8 位的合法 PNG，16 位或 APNG 会返回错误。 |
| `--scale N` | zoom、pick、compare、edit | 可选正整数，两轴共用。1 表示原尺寸。与 --target-long-edge 互斥。 |
| `--target-long-edge PX` | zoom、pick、compare、edit | 正整数，省略倍率参数时默认 1600。按当前区域的长边选择最接近目标的正整数倍率，等距取较小倍率，最低 1 倍。自动模式下回执含 target_long_edge 和实际 scale。 |
| `--region X,Y,W,H` | matrix、zoom、pick、compare、edit | 可选，逗号间无空格。使用原图全局坐标和正的宽、高，区域必须在原图内。省略即整张原图。 |
| `--out-dir DIR`、`--out-name FILE.png` | zoom、edit | 可分别指定。目录可自动创建，文件名只含名称。相同路径的预览会被替换。 |
| `--preview FILE` | compare、pick | 已经保存的预览。省略时读取默认位置。 |
| `--sample-limit N` | compare | 最多返回 N 个差异点，默认 8，0 表示只要总数。全图均参加比较。 |
| `--pick-source X,Y`、`--pick-preview X,Y` | pick | 均可重复或混用。前者为原图全局坐标，后者为放大图文件坐标。 |
| `--set-source X,Y=RRGGBBAA`、`--set-preview X,Y=RRGGBBAA` | edit | 至少一项，可重复或混用。六位色码补 FF，允许加 #，建议将整个值用引号包裹。指定点必须在选定区域内。 |
| `--edited-source FILE.png`、`--in-place` | edit | 前者使用新的编辑版路径，后者显式替换输入原图。默认新建原图同目录下的 NAME.edited.png。新建模式要求目标文件尚不存在。 |
| `--out NEW.zip` | licenses | 将内嵌的第三方许可归档导出到指定新文件，父目录需已存在。可直接阅读 skill 中相同的 references/third-party.zip。 |
| `--out NEW.json` | matrix | 写入新的 UTF-8 JSON 文件，父目录需已存在。标准输出仅返回 command、source、region、out。省略时标准输出包含矩阵本身。 |
| `--point X,Y`、`--region X,Y,W,H` | matrix | 均可重复。一项沿用原有矩阵结构；多项时 `regions[]` 内每项是独立的矩阵对象。先列所有点，再列所有矩形，同类保持输入顺序。多选输出到文件时回执给出 count 和 out。 |
| `--patch FILE` | edit、edit-matrix | UTF-8 JSON，可带 BOM。路径为 `-` 时读取标准输入。edit 的 patch 与旧式逐点参数二选一。 |
| `--source MATRIX.json --out NEW.json` | edit-matrix | 输入为包含 pixels 的单选区或整图矩阵。输出为新 JSON 文件，父目录须已存在。直接使用矩阵中的颜色，PNG 无需在场。 |

## 联合选区

所有选区和目标位置均使用原图坐标。输入图和目标图可以有不同尺寸。

| 参数 | 用法 |
|---|---|
| `--point X,Y`、`--rect X,Y,W,H` | 用于 copy 或 edit 的移动模式，至少一项，两种均可重复。点和矩形取并集，重叠处只计一个像素。 |
| `--out NEW.json` | copy 新建独立的剪贴文件，父目录须存在。后续可多次传给 --paste。 |
| `--move-by DX,DY` | 有符号的原图像素位移。正数向右、向下，负数向左、向上。先采样整组，再将原位清成 #00000000，最后落笔，支持新旧位置重叠。 |
| `--paste CLIP.json --at X,Y` | 读取 copy 的快照，将选区包围框的左上角放到 (X,Y)。每次 edit 接受一个落点。 |
| `--flip-x`、`--flip-y` | 在同一个包围框内左右或上下翻转，可叠加。搭配 --move-by 0,0 可在原地翻转。 |

一次 edit 选择一种输入方式：逐点设色、补丁、移动或粘贴。移动的起点和终点、粘贴的终点须完全落在画布或指定的 `--region` 内。越界返回码 2，源图保持原样。未选中的空隙保留目标颜色。选中的透明像素会覆盖目标，保留源 RGBA 的四个通道。

copy 返回 `out` 绝对路径和 `selection`。其中 `source_anchor` 是原包围框左上角，`size` 是 [宽,高]，`selected_pixels` 是并集像素数。edit 的 `selection` 另含 `action`（move 或 paste）、`target_anchor`、`flip_x`、`flip_y`。默认另存编辑版，打开 `preview.path` 或 `change_preview.path` 检查落点。移动的变化图可能同时包含移出和移入位置。

## edit 的自动局部预览

| 参数 | 默认值 | 作用 |
|---|---|---|
| `--preview-changes` / `--no-preview-changes` | 开启 | 显式启用或按次关闭变化预览，两者互斥。适用于 edit 的所有写入方式。 |
| `--padding N` | 2 | 四边各留 N 个原图像素，接受非负整数，0 表示紧贴变化框。裁到原图边缘；搭配 --region 时上下文可伸出编辑选区。 |

常规 `preview` 沿用原有取景。新增 `change_preview`，其 `status` 有三种值：

- `generated`：同目录保存编辑后局部图，文件名在常规预览的扩展名前加 `.changes`。返回绝对 path、放大图 width/height、changed_pixels、padding、未补边的 bounds 和补边后的 region。两个矩形均是全图坐标 `{x,y,width,height}`。显式 --scale 时局部图沿用该倍率，自动模式按补边后的区域重新选倍率。读取 change_preview.scale 可复现该视图。多处变化共用一个最小包围框。
- `unchanged`：输入和最终 RGBA 完全一致，changed_pixels 为 0。重复落笔后回到原色也属于这种情况。透明像素的隐藏 RGB 参与比较。
- `disabled`：本次关闭变化检测。与 unchanged 一样不输出局部图路径，同目录已有旧局部图不会被此调用更新。消费本次结果应读取 JSON 内的路径。

## 批量写入契约


补丁包含有序的非空 `ops` 数组，可附 `palette` 对象。每个操作采用下表一种结构。所有坐标都是从零开始的**原图全局坐标**，矩形格式是 `[x,y,width,height]`。命令带局部选区时，连同跳过格在内的完整操作区域都必须在选区内。

| 操作示例 | 含义 |
|---|---|
| `{"points":[[4,5],[9,5]],"color":"#3877AA"}` | 同色多点。不同颜色各写一项操作。 |
| `{"rects":[[1,2,2,3],[5,2,1,3]],"color":"transparent"}` | 同色填充多个矩形，可用于批量擦除。 |
| `{"at":[3,2],"rows":[".A","_A"]}` | 按 palette 解析等宽字符行；例如 `{"A":"#ABCDEF","_":"transparent"}`。点号固定表示保留该格，其他符号各是一个 Unicode 字符。 |
| `{"at":[3,2],"pixels":[[null,"#123456"],["transparent","#11223300"]]}` | 等宽二维 RGBA 网格。null 跳过；transparent 写入 #00000000；八位色码原样写入，包括透明像素的 RGB。 |

重叠位置依次写入，最后一次有效写入生效。任意操作无效，整批不提交；输入文件可重试。成功时 patch.operations 为操作数，patch.pixel_writes 包含重复和同色写入次数，跳过格不计入。旧式逐点模式保持原有 changes 记录和重复点检查。

`edit --patch` 也接受 `matrix --out` 或 `edit-matrix --out` 的完整导出文件，按记录的坐标回填全部像素（含透明像素）。多选区矩阵依 regions 顺序回填。要保留编辑后发生的其他 PNG 改动，用仅写指定格的 ops 补丁。批量读取多选区时对每个选区重新解码；频繁查询同一小块可合并成一个矩形。补丁与 edit-matrix 的输入 JSON 在内存中解析，大图优先导出局部矩阵。

## 原图数据契约

- `bounds` 扫描全图，以 alpha > 0 判定非透明。输出 `source.{path,width,height}`、`bounds.{left,top,right,bottom,width,height}`、`margins.{left,top,right,bottom}`。边界含首尾，宽为 right − left + 1。全透明图的 bounds 和 margins 均为 null。多个分离的可见物体共用一个包围框。
- `matrix` 默认导出整张图片。输出 command、source、region 和二维数组 pixels。源图绝对坐标 (x,y) 的色码为 `pixels[y-region.y][x-region.x]`。单行用 `--region 0,Y,图宽,1`，单列用 `--region X,0,1,图高`，将占位符换成整数。矩阵保留半透明及完全透明像素的四通道值。
- 两个命令可单独读取源 PNG，无需预览或倍率。仅需内容尺寸时直接调用 bounds。大矩阵优先用 matrix 的 --out 接口，让文件承载完整数据。

坐标原点为左上，x 向右，y 向下。设局部区域左上为 (ox, oy)，倍率为 s，预览点 (X, Y) 对应原图 (ox + X // s, oy + Y // s)。从原图点查询时，preview_xy 是预览块的左上角，preview_rgba 读取该点的实际值。preview_block_inclusive 的右、下端均包含在范围内。颜色输出为直通道 #RRGGBBAA，透明像素隐藏的 RGB 也参与比较。

默认预览名称为 NAME@Nx.png。有区域参数时为 NAME.X-Y-W-H@Nx.png。zoom 使用输入原图的目录和名称，edit 使用编辑后原图的目录和名称。调用方保留返回的路径、倍率和区域即可在下一次调用恢复映射。

## 读回、对比与编辑

~~~powershell
& $px zoom --source $src --scale 12 --region '3,4,8,3'
& $px pick --source $src --scale 12 --region '3,4,8,3' --pick-source '4,5'
& $px edit --source $src --scale 12 --region '3,4,8,3' --edited-source '.\work\cat-v2.png' --set-preview '13,14=534454FF' --set-source '9,5=534454FF'
& $px compare --source '.\work\cat-v2.png' --scale 12 --region '3,4,8,3'
~~~

- zoom、compare、pick、edit 成功读取图像时，主结果包括 command、scale、region、source、preview、out_dir、out_name。source 和 preview 均有绝对 path、width 和 height。比较遇到尺寸错误时 preview 报告磁盘文件的实际尺寸。
- pick 单点返回 selection，多点返回 selections 数组（先原图点、再预览点，同类保持输入顺序）。每项内含 selected_on、source_xy、preview_xy、source_rgba、preview_rgba、preview_block_inclusive。pick 仅读取指定点。需要验证整个方块时执行 compare。
- compare 先检查尺寸，再按行比较解码后的 RGBA。comparison.match 为布尔值。尺寸一致时 mismatch_pixels 为预览中有差异的像素数，samples 给出源坐标、预览坐标、期望色、实际色。尺寸错误时 reason 为 dimensions，同时返回 expected_width、expected_height，mismatch_pixels 为 null。
- edit 的 input_source 指明读取的原图，source 指向编辑后保存的文件，changes 报告各点的原图坐标、预览坐标、修改前后 RGBA。编辑按给定倍率和区域换算预览坐标。对来源不明的预览先 compare，确认它对应当前原图。
- 输出为静态 RGBA8 PNG，保留像素通道值。编辑索引色原图时会转为 RGBA。文本、EXIF 等附加元数据不会复制；灰度 ICC 在转为 RGB 时略去。需要保留原始文件时用默认另存。
- 普通 PNG 预览按行写出和比较。zoom/compare 保留选区的源像素，edit 需要读取整张原图；Adam7 解码另需完整帧。PNG 是顺序压缩格式，选区靠近底部时需要经过前面的源行。

## 状态与重试

| 退出码 | 含义 |
|---|---|
| 0 | 命令成功；compare 的像素完全匹配。 |
| 1 | 对比完成，尺寸或颜色有差异。读取 comparison，保留差异供诊断。 |
| 2 | 参数、路径、解码或保存错误，JSON 的 error 给出原因。 |

写入时先生成同目录临时文件，再逐个替换。若原图已经保存而预览提交失败，error 会指明已保存原图路径；用该路径、原倍率、原区域和预览输出路径运行 zoom，再运行 compare。操作系统会在写入进程退出时清除写锁。两个写入操作竞争同一输出时，后启动的调用可在前一次结束后重新读取文件并重试。
