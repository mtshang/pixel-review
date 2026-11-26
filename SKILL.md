---
name: pixel-review
description: 在 Windows x64 上按项目美术规范从零绘制或迭代 PNG 像素素材。用已认可参考图校准造型，通过原尺寸、放大细节和场景组合图评审方案；也支持分层工程、地砖和动画观察。
---

# 像素画观察与修改

从本文件所在目录定位 `scripts/pixel-review.exe`。传给程序的图片路径相对于调用者的当前目录。它输出一行 UTF-8 JSON，适合直接解析。全量参数和输出契约见根目录的 [CLI 接口总览](cli.md)。

## 使用时检查版本

开始使用本 skill 时，查看本文件所在目录下的 `local/version-check.txt`。用 UTC 比较该文件的最后写入时间与当前时间。首次使用、记录时间无效或已满 72 小时时，在 skill 根目录运行下面的命令。其余时间直接继续本次任务。用户明确要求检查版本时立即运行。

~~~powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File ".\scripts\check-version.ps1"
~~~

读取脚本输出和退出码。若 `Update needed: Yes`（1），向用户报告本地版本、云端版本及脚本返回的 GitHub 链接，由用户决定是否更新。若为 `No`（0），继续工作即可。每次检查结束后，将本次输出写入上述记录文件，按需创建 `local` 目录。若为 `Unknown`（2），照常完成当前任务；下次使用时，距离这次失败满 24 小时即可重试。若安装目录只读，在当前会话内记住检查时间和结果。

## 先确定本次图片输出目录

首次要写出图片时，先问用户「这次的成品、放大预览和评审图保存在哪个文件夹？」等用户指定后再写。若用户已在请求中给出输出文件路径、目录，或明确约定本项目固定使用某个目录，就沿用该位置。只查询尺寸、颜色等数据时可直接读取原图。参考图库和图片输出目录分别确认，候选稿与预览保存在本次输出目录。

将选定目录解析为绝对路径。zoom 传 `--out-dir DIR`；edit 传 `--edited-source DIR/新版.png --out-dir DIR`，这样主预览与自动局部预览也落在这里。canvas preview/export、review-sheet、animation 等命令使用目录下的 `--out` 路径，动画横排图会跟随主输出。pick 和 compare 读取先前返回的 `preview.path`。在同一次改稿中复用这个目录，保留上一版所需的文件。

需要区分画布尺寸与人物尺寸时用 `bounds --source FILE`。需要原图坐标和多点颜色时用 `matrix --source FILE [--region X,Y,W,H]`。大矩阵可写入 `--out NEW.json`。这两个命令可独立调用。

## 项目美术资料

第一次为某个游戏从零画素材时，先从该项目的资料或用户给的路径找到已有规范和参考图库。已有位置就沿用；尚未约定时，问一次「这个游戏的美术参考文件夹放在哪里？可以放在项目目录下的『美术参考』。」用户选定后，在那里创建 `外部参考/`、`本项目已确认/`，并用随包的[美术规范模板](assets/art-guidelines.md)初始化 `美术规范.md`。保留既有文件和目录组织。记下资料路径供后续任务复用，例如在项目已有的美术索引中补一个相对链接。暂时只做一次试画时也可直接使用用户当次给的参考。

开始绘制前，读适用的规范，实际打开最相关的已确认图。核对与本次有关的画布和可见轮廓尺寸、视角、脚底行、色表、描边或特征。外部参考按用途登记，例如「只参考斗篷剪影」。把从图中推测的规律记为本次判断；要升为全项目规则时以用户确认为准。用户明确认可某个版本后，再把该版原尺寸图的副本和认可范围登记到「本项目已确认」。若只认可发型，就记录「仅发型」，以后比较衣服时仍需另找依据。候选图、评审拼图和修改稿留在工作目录，保留已认可的旧版及其适用说明。

## 从零绘制到交付评审

1. **先画轮廓和大色块。** 用透明画布、字符网格和少量主色画出姿态与结构。角色先定头身比、重心和四肢分离，物件先定朝向和最能辨认它的外形。按游戏显示倍率看一次，再放大整图对照已确认图。
2. **先选形，再细化。** 判断最小显示尺寸下能否辨认用途、朝向和动作。轮廓读不出来就先改色块。若有两个会明显改变造型的方向，可保留两张候选，用下面的组合图对比。能依据项目规范自行判断时继续完成；只有确需用户选方向时才中途询问。
3. **加入细节并放回游戏画面。** 在选定草稿上加五官、衣服分界或材质，按下节的局部与整图流程复看。使用项目实际背景、窗口倍率和原图坐标做场景预览；检查遮挡、对比和相对尺寸，再在原图上精修。缺少真实背景时可先交角色双视图，使用代用场景要说明来源。
4. **交付可直接指认的版本。** 调用 `review-sheet --source A.png --source B.png --display-scale N --scene ROOM.png --at X,Y --out NEW.png` 并打开结果。每列从上到下为游戏倍率、细节、同倍率场景。带遮挡的房间可用 `--foreground 栏杆.png`，或用 `--scene 工程目录 --replace-layer 角色` 沿用工程层序。比较不同尺寸时读回执里的锚点和偏移。影子或衣摆低过脚时按列传 `--anchor X,Y` 指定同一只脚。单方案只传一个 source；有上版时加 `--before OLD.png`，它固定排在第 1 列。给用户同时提供候选原图和评审图，说明编号对应哪版、要比较什么。收到反馈后从指名的版本继续，沿用相同背景、取景、倍率和落点核对改动。[组合预览参数](cli.md#方案评审图)

## 创作与修改时的观察流程

先按实际显示尺寸看一遍 → 放大看整图 → 放大看要改的局部 → 在原图上编辑 → 用与编辑前相同的倍率和取景复看局部和整图。

1. **按实际显示尺寸看一遍。** 打开原图，已知游戏内的显示尺寸和背景时按该条件观察。先辨认轮廓、姿态和表情，记下这次要解决的视觉问题。
2. **放大看整图。** 运行 `zoom --source FILE` 并打开 `preview.path`，观察目标部位与整个人物或画面的比例和位置关系。默认按当前视野长边最接近 1600 像素选正整数倍率，可用 `--target-long-edge PX` 自定。原图长边超过目标时以 1 倍观察。保留整图预览路径及回执中的 `scale`。
3. **放大看要改的局部。** 根据整图选择原图坐标下的 `--region X,Y,W,H`，再次运行 zoom 并打开局部图。修眼睛时可带上刘海、鼻尖和下巴，便于判断目光和脸部轮廓。工具按局部宽高独立选倍率。保留这次的区域、倍率和预览。需要定位时用 `pick`，可重复、混用 `--pick-source` 和 `--pick-preview`。后者接受预览文件自身的像素坐标。
4. **在原图上编辑。** 给 `edit --source` 传实际尺寸的 PNG。少量点用 `--set-source`，成块绘制用 `--patch`。若按局部预览点选，用该预览的区域和实际倍率搭配 `--set-preview`。保存编辑版，用返回的 `source.path` 继续。
5. **按原视野复看局部和整图。** 对编辑版先用步骤 3 保存的区域与倍率生成局部图，再用步骤 2 的倍率生成整图。分别打开两组前后预览，先核对局部色块，再看改动放回整幅画后是否协调。用不同输出路径保留前后版本。需要验证最近邻映射时运行 `compare`，交付游戏素材时也可按实际使用尺寸复看。

## 地砖与动画观察

制作地砖时用 `tile-preview --source TILE --repeat 5,5 --out NEW.png` 实际打开看连续接缝与重复斑点。检查墙壁的直边、内角和外角用 `--layout FILE.json` 表达真实相邻关系。加 `--overlay ROLE.png --at X,Y` 观察角色与房间的比例。需要检查工程里未保存的层时，先用 canvas export 导出该层的原尺寸快照。

1. 每个朝向和动作可保存一份配置，例如 `animation-config --frame A.png --frame B.png --fps 6 --baseline 37 --out walk.json`。帧按输入顺序播放，配置内的相对路径以该 JSON 所在目录为起点。Y 为鞋底最后占据的原图行，高 40、底部留 2 格时 Y=37。
2. 调用 `animation --config walk.json --out NEW.apng`。通过静态图片入口打开回执里的 **strip.path**，沿编号从 1 开始逐帧看轮廓、支撑脚和跨步程度，并比较最后一帧到第一帧的衔接。每帧在横图内的位置是 frames[].strip_region。要判断节奏或滑步，在支持 APNG 的浏览器中打开 **preview.path**，观察几个循环。只能看到静态图时，把结论限于可见姿势和位置。
3. 发现某帧可疑时用 `onion --config walk.json --index N --out detail.png`。前一帧的变化显示青色，后一帧显示洋红色。检查 frames[].bounds、bottom_y 和 bottom_delta，结合衣摆、影子判断最低可见像素是否就是脚。复看时沿用同一配置与倍率。临时隐藏基线用 `--no-baseline`。[数据格式和示例](cli.md#地砖与动画预览)

## 扩边与裁切

普通 PNG 用 `resize --source FILE --right 4 --baseline 37 --out NEW.png`。常驻工程用 `canvas resize --canvas A --right 4`。若要在角色层新增的四列继续画手臂，加 `--pad-layer 角色`，也可重复指定多个层。按原图坐标取矩形用 `--crop X,Y,W,H`。这些操作保持像素原尺寸。

左、上边界变化后，旧坐标加 resize.origin_shift 得到新坐标。动画帧需要相同的取景，对每帧应用同样的边距，再用返回的 new_baseline 更新动画配置。工程裁切保留画布外图层像素，可通过反向扩边重新看到。检查预览后手动保存，或用 canvas undo 整步恢复。[边距和图层坐标](cli.md#画布扩边与裁切)

## 跨图摆放与手动保存

从零绘制可直接 `canvas create --canvas A --size 20,40 --out NEW.png` 创建透明底图并进入常驻会话。仅生成透明 PNG 用 `create --size W,H --out NEW.png`。

需要独立回退的视觉决定分成不同的画布命令提交，例如头发一份 edit、眼睛另一份 edit。用 `canvas undo --canvas A` 回退一笔，`canvas redo --canvas A` 恢复，随后在原视野复看。status 的 undo_steps、redo_steps、dirty 用于判断可用历史和是否需要保存。最近 32 步限当前会话，撤销后作出新的有效修改会清除重做分支。

一组素材沿用同一 JSON 色表时，用 `canvas palette-use --canvas A --palette FILE` 载入，随分层工程保存。临时字符稿可传 `--palette FILE`。检查多张磁盘 PNG 用 `palette-check --palette FILE --source A.png --source B.png`，读 ok 和每张图的 samples；已确认的特殊色按次传 `--allow RRGGBBAA`。批量修正用 `recolor --palette FILE --source A.png --source B.png --map '旧色=@H' --out-dir NEW_DIR`，将旧色换成实际色码。检查未保存图层或做可撤销换色用 `canvas palette-check`、`canvas recolor`，均须指定画布名。[命令与颜色规则](cli.md#空白画布撤销和共享调色板)

若要保留下层地面或原角色，先按下节新建独立层，再粘贴或落笔。直接贴在 Base 上会把颜色写进底图。

1. 每张要连续操作的图执行一次 `canvas open --canvas NAME --source FILE`。成功返回时已就绪。给同时打开的图取不同的名字，记录 `canvas`、`pid` 和当前 `source.path`。跨工作目录调用时用剪贴文件和预览文件的绝对路径。
2. 在来源画布执行 `canvas copy --canvas A --rect X,Y,W,H --clip NEW.json`。可混选多个 `--rect` 和 `--point`。复制的是此刻的内存画面。剪贴文件可重复使用，`canvas close --canvas A` 后仍有效。
3. 在目标画布执行 `canvas paste --canvas B --clip FILE.json --at X,Y`。重复 `--at` 可一次摆到多处。粘贴地图物品时，如希望透明背景露出地面，加 `--skip-transparent`。操作会修改内存中的原尺寸画布。
4. 需要观察当前未保存的画面时用 `canvas preview --canvas B --region X,Y,W,H --out NEW.png`，省略区域即整图。支持与 zoom 相同的倍率参数。打开返回的 `preview.path`，前后对照用不同的新路径。普通 zoom 读取磁盘版本。
5. 用 `canvas status --canvas B` 查看保存目标和 `dirty`。普通单层 PNG 可用 `canvas save --canvas B --out NEW.png` 另存并切换保存目标。分层工程首次用 `save --project NEW_DIR`，之后用 `save`。最后执行 `canvas close --canvas B`。只有明确要放弃未保存内容时才传 `--discard`。[常驻画布命令契约](cli.md#常驻画布)

## 分层观察与绘制

以下动作均以 `canvas 动作 --canvas NAME` 调用。一份工程共用一个进程。打开现有工程时把目录传给 `open --source DIR`。从 PNG 开始时原图成为 Base。

1. 把需独立移动的素材导入新层，例如 `layer-add --layer 人物 --source sprite.png --at 22,4`。从零画用 `layer-add --layer 人物 --size 20,40 --at 22,4` 建透明层。新层位于顶端并成为活动层。
2. 用 `status` 核对从底到顶的 layers、active_layer、各层尺寸与 at。观察整体用 `preview --out NEW.png`。观察单层用 `preview --layer 人物 --region X,Y,W,H --out NEW.png`。合成视图的 region 用画布坐标，单层视图及 edit、copy、paste 用图层自身左上角为原点。两种视图分别记录取景与倍率。
3. 用 `edit --layer 人物 --patch patch.json` 提交点、矩形或字符稿，透明擦除只影响该层。用 `layer-set --layer 人物 --at 50,4` 移到新的绝对位置。需要检查被遮住的背景时，可单独预览 Base，也可用 `layer-set --layer 人物 --visible false` 暂时隐藏人物，再用 true 恢复。
4. 复看同视野的局部与合成图。首次执行 `save --project NEW_DIR`，以后一律可用 `save` 更新当前工程。交付原尺寸 PNG 用 `export --out NEW.png`；只交付素材用 `export --layer 人物 --out NEW.png`。导出后仍需 save 才会保存工程编辑。[图层参数与坐标约定](cli.md#分层工程)

平移已有像素时用 `edit --source FILE --scale N --point X,Y --rect X,Y,W,H --move-by DX,DY`。点和矩形可混用、重复，整组只移动一次。复制时用 `copy --source FILE --point X,Y --rect X,Y,W,H --out NEW.json`，之后用 `edit --source TARGET.png --scale N --paste NEW.json --at X,Y`。读取回执的 `selection.source_anchor`、`target_anchor`（编辑时）和 `size` 核对落点。锚点是整组包围框左上角，那里可能没有选中像素。移动或粘贴时可加 `--flip-x` 或 `--flip-y`。试落点时用新的 `--edited-source` 写候选版，再打开本次预览。[选区参数及透明语义](cli.md#联合选区)

每次 `edit` 默认按实际 RGBA 差异另出变化预览，`generated` 时可打开 `change_preview.path` 定位落笔。这个框随改动变化，步骤 5 的前后对照仍用编辑前保存的两个视野。默认四周留 2 个源像素，`--padding N` 调整留白，`--no-preview-changes` 关闭本次自动取景。重交整张字符稿时，传入上一版成品作为 `--source`。同色写入和最后恢复原色的点不计入差异。`unchanged` 和 `disabled` 状态没有本次局部图。校验或点选变化预览时使用 `change_preview.region`、`change_preview.path` 和 `change_preview.scale`。

仅观察时用 `zoom`、`compare` 或 `pick`。已有预览用自定义名称时，读取操作传 `--preview`，局部预览同时传原先的 `--region`。保存多个版本时使用不同的输出文件名。

一次检查多个位置时用 `matrix --point X,Y --region X,Y,W,H`，两种参数均可重复。直接修改单个导出颜色表用 `edit-matrix --source MATRIX.json --patch FILE --out NEW.json`，仍用全图坐标；再用 `edit --source SOURCE.png --scale N --patch NEW.json` 落回图片。[批次格式与坐标契约](cli.md#批量写入契约)

一份修改单可混合多组点、矩形和网格。在 `rows` 中用 `.` 跳过、用映射到 `transparent` 的字符擦除；在 `pixels` 中分别用 `null` 和 `"transparent"`。验证这两种语义时选取原先有颜色的位置，分别检查跳过位置的 RGBA 保持原值、擦除位置变成 `#00000000`。
