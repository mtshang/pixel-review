# Pixel Review · 像素画观察与修改

当一只 14×10 像素的小猫在屏幕上太小，看不清眼睛时，Pixel Review 可以把它清晰地放大，让你找到某个色块对应的原图像素。你可以修改那一格，再回到原尺寸判断表情。这是一个 Windows x64 命令行工具，也是一份供 AI 使用的 Codex skill。

![示例小猫，每个源像素放大为 8×8 色块](examples/cat@8x.png)

第一次使用可以从[按功能学习的命令手册](cliWiki.md)开始。需要测量角色实际宽高时用 `bounds`，读取整图、一行、一列或一个选区的颜色时用 `matrix`。成块画图用 `edit --patch`，一份修改单即可为多个点或矩形设色、绘制字符图案、直接写入 RGBA 网格；`edit-matrix` 可先修改导出的颜色表，再按坐标贴回原图。所有参数和 JSON 字段见[根目录接口总览](cli.md)。

连续摆放地图物件时，可以把素材和地图打开为两个具名画布，复制后关掉素材，在地图上多次粘贴、查看局部，最后手动保存。[常驻画布练习](cliWiki.md#五同时打开素材和地图)

把人物独立放在地板上时，可以保存为分层工程。移动人物只修改它在画布上的位置，底下的地砖仍保留原来的颜色。工程支持导出合成 PNG 和每个图层的原尺寸素材。[分层绘制示例](cliWiki.md#六把可以独立修改的内容放进图层)

从零画角色时，可以直接创建透明画布，按笔撤销或重做，并让正面、背面和其他素材共用一份色表。[创作、撤销与统一配色练习](cliWiki.md#七从零起稿试错和统一配色)

制作地图时，可以把一块地砖铺成整片，或把内角、外角和直墙拼成房间，再放入角色看比例。制作动作时，可以把多张 PNG 按帧率循环播放，用前后帧叠看和脚底线检查跳动。[地砖铺设与拼接练习](cliWiki.md#八看地砖铺满以后墙角怎样接起来)

每个动作可以用一份配置记住帧序、帧率和脚底线。生成播放文件时会同时得到一张带帧号的横排 PNG，即使看图窗口只显示静态画面，也能看清每一步。角色的手臂需要更多空间时，可以只向右加几格，指定需要继续绘制的图层一起扩边。[动画练习](cliWiki.md#九看动画的节奏重影与脚底) · [伸手动作扩边练习](cliWiki.md#十给伸手动作留空间保持鞋底对齐)

让 AI 从零画游戏素材时，可以给项目准备一个参考文件夹，收录外部参考、你已认可的原图，以及每张图具体认可了什么。AI 先用轮廓和大色块定形，再补细节。交稿时可以把候选按编号并排，分别看游戏显示大小、放大细节和实际场景。扩边后的不同尺寸也能按脚底对齐；场景默认跟随游戏显示倍率，可叠加前景或直接读取分层工程观察遮挡。你可以直接指出第几列的哪个部位要保留或修改。[参考图库和方案评审练习](cliWiki.md#十一怎样让-ai-按同一套画法出稿给你看图)

## 用小猫试一次

要把几处分开的像素一起挪动，可以混选点和矩形。比如将两只眼睛上移一格，或把眼睛和眉毛存成一份剪贴文件，贴到另一张表情图。两只眼睛之间未选中的格子会保留目标图的颜色。[移动、镜像和粘贴的练习](cliWiki.md#四选中已有像素再移动或复制)

随包的两张示例图是教程输入的小猫和本页展示的八倍预览。AI 在首次输出图片前会询问保存目录。手动练习时，以下命令把小猫复制到 Git 已忽略的 work 目录，预览和改稿都写在这里。个人色表、动画配置等如需放在工具仓库内，可存入同样被忽略的 local 目录，或在仓库根目录命名为 `*.local.json`。配置按命令参数显式读取。

在本文件所在目录打开 PowerShell，只需运行 `scripts\pixel-review.exe`。

~~~powershell
$px = '.\scripts\pixel-review.exe'
$cat = '.\work\cat.png'
$null = New-Item -ItemType Directory -Force '.\work'
if (!(Test-Path -LiteralPath $cat)) { Copy-Item -LiteralPath '.\examples\cat.png' -Destination $cat }
& $px zoom --source $cat --scale 8
~~~

打开输出中的 `preview.path`，也就是 `work\cat@8x.png`。原图宽 14、高 10，预览宽 112、高 80。查询预览上靠左的那只眼睛：

~~~powershell
& $px pick --source $cat --scale 8 --pick-preview '35,42'
~~~

这次会得到原图坐标 `(4,5)`、颜色 `#392E39FF`。整块眼睛在预览里占据横坐标 32 至 39、纵坐标 40 至 47。坐标从 0 开始。也可以反过来用 `--pick-source '4,5'` 查询这块范围。

将这只眼睛改成蓝色，再校验保存的预览：

~~~powershell
& $px edit --source $cat --scale 8 --set-preview '35,42=3877AAFF' --edited-source '.\work\cat-blue.png'
& $px compare --source '.\work\cat-blue.png' --scale 8
~~~

打开 `work\cat-blue.png` 和 `work\cat-blue@8x.png`，分别看真实尺寸和放大细节。`comparison.match` 为 `true` 表示预览中的每个像素都与编辑版原图对应。重复试不同配色时，把 `--edited-source` 改为新的文件名。想在同一张编辑版上继续修改，可以将它用作 `--source` 并显式加 `--in-place`。

每次编辑也会默认生成一张改动处的局部图，位置在 `change_preview.path`。用 `--padding N` 调整四边留白，或用 `--no-preview-changes` 关闭当次局部预览。

## 大图、透明背景与输出

- 对局部细节，可加 `--region '100,200,64,64'`，只放大原图从 (100,200) 开始的 64×64 区域。查询、编辑、校验该视图时沿用相同的区域和倍率，返回的原图坐标始终是全图坐标。
- 放大使用最近邻，两轴倍率相同，保留透明度及完全透明像素的隐藏 RGB。支持静态 PNG，包含索引色、灰度、RGBA 和交错 PNG，每通道最多 8 位。编辑后的文件为 RGBA8。
- 用 `--out-dir` 和 `--out-name` 指定预览的位置与文件名。已有的同名预览会被替换。读取自定义预览时加 `--preview`。默认另存编辑版，可保留原始的调色板和附加元数据供日后使用。
- 程序输出一行 JSON。compare 遇到不同像素会列出两份文件、差异数量、样本坐标以及实际和期望颜色。[完整命令与返回值](cli.md)

## 检查版本

在工具根目录打开 PowerShell 或命令提示符，运行下面这条命令。脚本读取本地根目录的 version.json，并从 GitHub 的 main 分支读取同名文件。脱离工具目录调用时，把命令末尾的脚本路径换成绝对路径。

~~~powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File ".\scripts\check-version.ps1"
~~~

输出使用英文。Update needed 为 Yes 表示云端较新（退出码 1），No 表示本地相同或更新（退出码 0），Unknown 表示本次无法完成比较（退出码 2）。单独复制 EXE 的用户如需检查版本，也要带上根目录的 version.json 和 scripts 下的检查脚本，保持目录结构。

## 安装空间与许可

单独使用时，复制 `scripts/pixel-review.exe` 即可。1.12.0 的 Windows x64 程序为 **1,490,944 字节（约 1.42 MiB）**，额外运行环境的安装空间为 **0 字节**。处理时仍需为图片、临时输出和编辑副本预留空间。

Rust、Cargo、PNG 编解码库和 Python 均无需另装。Rust 编译器的后续升级或卸载不影响这个已构建的程序。若以后希望获得依赖库的修复，可用更新后的依赖重新构建和验证。

本次构建把 Rust 代码和 PNG 依赖链接进 EXE，并内嵌各组件的许可文本及版权声明。png 0.18.1 采用 MIT 或 Apache-2.0；归档包含本次构建的依赖清单、Rust 标准库声明和 GNU 工具链运行库的许可及例外。分发时可保持 EXE 原样，单文件仍能导出这些文本。[png 项目](https://github.com/image-rs/image-png) · [Rust 链接说明](https://doc.rust-lang.org/reference/linkage.html) · [本版许可归档](references/third-party.zip)

~~~powershell
.\scripts\pixel-review.exe licenses --out .\work\third-party-notices.zip
~~~

## 实测耗时

下表为 1.0 版本在本机 通过命令行独立启动三次的中位耗时，包含启动及文件读写。样本为单色地板加一只小猫。纹理复杂度、磁盘和查看区域的位置都会影响结果。

| 原图尺寸 | 观察范围与倍率 | 生成预览 | 逐像素校验 |
|---|---|---:|---:|
| 100×200 | 全图，8 倍 | 43 ms | 19 ms |
| 2000×2000 | 全图，2 倍 | 51 ms | 62 ms |
| 10000×10000 | 右下角 60×60，8 倍 | 172 ms | 206 ms |

1.1 版另用一张 2048×2048 的透明 RGBA 图（中央为 1024×1024 实色矩形）测试了原图查询。预热后交替运行七轮，独立进程的中位耗时为：直接查边界 9.5 ms，完整矩阵导出到 JSON 文件 47.0 ms。后者尚未包含重新读取 JSON、计算边界的时间。

## AI 使用与重新构建

把整个 `pixel-review` 文件夹放入 Codex 的个人 skills 目录后，可用 `$pixel-review` 调用。[AI 工作流](SKILL.md)和[CLI 接口说明](cli.md)包含调用例子、坐标约定及错误处理。

源码、`Cargo.lock` 和构建参数均随附。发布版采用 Rust 1.100.0、目标 `x86_64-pc-windows-gnu`，构建时需有该目标的 64 位 GNU 汇编器和链接工具可用。在本目录运行：

~~~powershell
cargo +stable-x86_64-pc-windows-gnu build --release --locked --target x86_64-pc-windows-gnu
~~~

构建产物位于 `target\x86_64-pc-windows-gnu\release\pixel-review.exe`。更新依赖或工具链时同步更新 `references/third-party.zip` 后重建。开发验证使用 Python 3 与 Pillow，将待测 EXE 放在 `scripts` 下运行 `python tests/cli.py -v`。测试会独立解码像素、核对坐标、编辑与透明通道，并校验内嵌许可归档。
