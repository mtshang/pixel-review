# Pixel Review 命令学习手册

如果你想知道“角色有多高”，从 `bounds` 开始。如果你想把眼睛放大看，用 `zoom`。如果你想知道一排刘海用了哪些颜色，用 `matrix`。这三件事都可以直接用一张 PNG 开始。

## 先认识画布、人物和预览

拿这次的角色举例。**画布**宽 20、高 40，是游戏实际读取的图片。**人物**占其中 16×32 像素，剩下的是透明留白。**预览**可以放大到 240×480，方便看头发和衣摆。游戏里按 20 像素为一米计算，人物在原图上依然高 1.6 米。

下面的练习使用随工具附带的 14×10 小猫。在本手册所在的目录打开 PowerShell，先执行：

~~~powershell
$px = '.\scripts\pixel-review.exe'
$cat = '.\work\cat.png'
$null = New-Item -ItemType Directory -Force '.\work'
if (!(Test-Path -LiteralPath $cat)) { Copy-Item -LiteralPath '.\examples\cat.png' -Destination $cat }
~~~

`& $px` 表示运行工具。跟在后面的第一个词选择功能，`--source` 指定要读取的图片。图片左上角是坐标 (0,0)，向右 x 增大，向下 y 增大。小猫最右一列是 x=13，最后一行是 y=9。

## 一、读取原图

### bounds · 量出画布里可见内容的宽高

~~~powershell
& $px bounds --source $cat
~~~

结果中，`source.width` 和 `source.height` 是画布宽高。`bounds` 是能够包住所有非透明像素的最小矩形。`margins` 是它与画布四条边之间的距离。小猫碰到了四边，所以四个留白都是 0。

换成上面的人物图，输出的左右边界是 2 和 17，上下边界是 6 和 37，两端都算进去。因此人物高为 37 − 6 + 1 = **32**。哪怕某个像素的透明度只有 1/255，也会算进边界；整张图完全透明时，`bounds` 和 `margins` 返回 `null`。如果原图已经铺满不透明地板，测到的是整张画布，需要人物单独的透明 PNG 才能用这个方法量身高。

### matrix · 把像素读成带坐标的颜色表

~~~powershell
& $px matrix --source $cat                         # 整张小猫
& $px matrix --source $cat --region '4,5,3,2'       # 从 (4,5) 起读宽 3、高 2 的矩形
& $px matrix --source $cat --region '0,5,14,1'      # 完整的第 y=5 行
& $px matrix --source $cat --region '4,0,1,10'      # 完整的第 x=4 列
~~~

`--region` 的四个数字依次是**起点 x、起点 y、宽、高**。第二条命令会在 `pixels` 字段里给出：

~~~json
[
  ["#392E39FF", "#EBA45CFF", "#EBA45CFF"],
  ["#FFE1A2FF", "#EBA45CFF", "#D67B80FF"]
]
~~~

每个内层数组是一行。第一格在原图的 (4,5)，右下角粉色那格在 (6,6)。`pixels[1][2]` 表示从选区起点往下 1 格、向右 2 格。颜色依次写红、绿、蓝、透明度，每个通道占两位十六进制。末尾 FF 表示完全不透明，00 表示完全透明。

大图可直接存成文件，终端只显示保存位置和选区。文件名需要是新的，输出目录要已存在：

~~~powershell
& $px matrix --source $cat --out '.\work\cat-colors.json'
~~~

## 二、观察和核对预览

### zoom · 将每个像素复制成一个方块

想先看整张画，再看其中的眼睛，可以让两张预览各自选倍率。在本手册目录设置好上面的 `$px` 和 `$cat` 后运行：

~~~powershell
& $px zoom --source $cat
& $px zoom --source $cat --region '3,4,8,3'
& $px zoom --source $cat --region '3,4,8,3' --target-long-edge 640
~~~

第一条按默认目标长边 1600 像素放大整只小猫，宽 14、高 10，选 114 倍，得到 1596×1140。第二条对 8×3 的局部单独选 200 倍，得到 1600×600。第三条把局部的目标改为 640，选 80 倍，得到 640×240。打开各次返回的 `preview.path`，`scale` 是实际倍率。

选择规则是让取景范围的长边乘以正整数后尽量接近目标。上下两个候选离目标一样远时取较小倍率。例如 20×40 的角色整图按默认值取 40 倍，恰好得到 800×1600。若想得到 480×960，可以用 `--target-long-edge 960`。整图已经是 3000×2000 时保留 1 倍和全部像素，截取其中一张脸时按脸部区域重新选倍率。

编辑前分别记下整图和局部的 `region`、`scale`。编辑后把 `--source` 换成编辑版，重用各自的取景和倍率，依次看局部与整图。想直接指定倍率时，使用下面的 `--scale`。一次调用在 `--scale` 和 `--target-long-edge` 中选一项，两项都省略即按默认目标计算。

~~~powershell
& $px zoom --source $cat --scale 8
~~~

打开返回的 `preview.path`。你会得到一张 **112×80** 的新 PNG，小猫原来的一个像素在这张图中占 **8×8** 格。`--scale` 是正整数，横纵共用，1 表示原尺寸。可以只放大一块区域，并自己命名输出：

~~~powershell
& $px zoom --source $cat --scale 8 --region '3,4,8,3' --out-dir '.\work' --out-name 'eyes.png'
~~~

这个区域宽 8、高 3，输出就宽 64、高 24。`--out-dir` 可自动建目录，`--out-name` 只填文件名。再次写入同名预览会更新它。

**什么时候会需要“缩放并放置到画布”？** 假设手里的人物高 40 像素，游戏要求它实际高 32 像素，画布仍是 20×40，鞋底还要留两格。这个任务要改变角色在成品里占多少格，并把它放到指定位置。`zoom` 只负责生成整数倍观察图，当前版本的角色缩放和放置仍由绘图程序完成。

### pick · 从预览上的一点找回原图像素

~~~powershell
& $px pick --source $cat --scale 8 --pick-preview '35,42'
& $px pick --source $cat --scale 8 --pick-source '4,5'
~~~

第一条查询小猫的眼睛，读回原图 (4,5)；第二条从同一原图像素出发，查出它在预览中覆盖 x=32…39、y=40…47。两个参数可重复、可混用。如果预览另存为 `eyes.png`，用 `--preview '.\work\eyes.png'` 指定它，并沿用生成时的 `--region '3,4,8,3'` 与倍率；此时预览坐标从局部图片的左上角起算。

一次核对多处修改时，可以把多个原图点和预览点放在同一条命令里。比如同时检查小猫的两只眼睛和一个放大像素：

~~~powershell
& $px pick --source $cat --scale 8 --pick-source '4,5' --pick-source '9,5' --pick-preview '35,42'
~~~

多点结果在 `selections` 数组里，先列原图点，再列预览点。需要读取连续的长方形色块时，用 `matrix --region`，可重复多次。

### compare · 检查每一格是否放大正确

~~~powershell
& $px compare --source $cat --scale 8
& $px compare --source $cat --scale 8 --region '3,4,8,3' --preview '.\work\eyes.png' --sample-limit 3
~~~

`comparison.match` 为 true 表示尺寸和全部 RGBA 都符合映射。`--sample-limit` 控制最多显示几例差异，默认 8；设为 0 仍会统计所有差异。要判断角色比例，用 bounds 测量，再打开实际尺寸的图片看效果。

## 三、改动原图像素

### edit · 在原图改色，同时生成新预览

~~~powershell
& $px edit --source $cat --scale 8 --set-source '4,5=3877AAFF' --set-preview '74,42=3877AAFF' --edited-source '.\work\cat-blue.png'
~~~

`--set-source` 用原图坐标，`--set-preview` 用预览坐标，两者都把指定颜色写回对应的原图像素。这条命令一次将两只眼睛改蓝。可重复使用这两个设色参数；这种逐点调用中，同一源像素一批只指定一次。六位颜色默认完全不透明，八位颜色能明确指定透明度。

`--edited-source` 指定一个新的文件名；省略时默认生成 `cat.edited.png`。如果明确要在已有的编辑版上继续改，用 `--in-place` 替换该输入文件。预览的目录和名称仍能用 `--out-dir` 和 `--out-name` 控制。

之前说的**“创建草稿的绘制操作”**，具体是从一张空白透明图开始，把已有头部复制到一个位置，用几行像素画出半边衣裙，再镜像补上另一半。`edit` 已经能把已有 PNG 上的一批指定像素改色。空白起稿使用 create 或 canvas create，粘贴与镜像见后面的选区练习。

### 改完自动看局部：眼睛改了几格，预览就跟到哪里

假设你把整张字符稿重交了一遍，只改了眼睛。把**上一版角色 PNG** 作为 `--source`，这样本次取景的基准就是你刚才看到的画面。程序会在编辑结束后找出最终颜色确实改变的格子，框住它们，在四边各加 **2 格** 上下文，再放大。指定 `--scale` 时两张图共用倍率；自动模式按补边后的局部尺寸独立选倍率，结果见 `change_preview.scale`。额外的局部图和常规预览都会保存。

用随附小猫试一次。下面把左眼改蓝，留白调到 3 格：

~~~powershell
$r = & $px edit --source $cat --scale 8 --set-source '4,5=3877AAFF' --padding 3 --edited-source '.\work\cat-eye-detail.png' | ConvertFrom-Json
$r.change_preview
~~~

这次 `bounds` 从 (4,5) 起，宽高各 1。补边后的 `region` 从 (1,2) 起，宽高各 7，打开 `change_preview.path` 就能看到一张 56×56 的眼部预览。打开 `preview.path` 可以看整只小猫。两张图仍可分别用 `compare` 校验；校验局部图时传它自己的区域。

- 要让预览紧贴改动，用 `--padding 0`。接近画布边缘时，留白会在边缘截断。
- 只需要常规预览时，加 `--no-preview-changes`；要在脚本里写明启用，可加 `--preview-changes`。
- 如果这次只重复写入原色，状态为 `unchanged`。看本次结果时，以返回的路径为准。若同时改了眼睛和鞋子，预览会框住两处及中间的区域。

### 一次读出几个点和几块区域

~~~powershell
& $px matrix --source $cat --point '4,5' --point '9,5' --region '3,4,3,2' --region '8,4,3,2'
~~~

这一次会返回四份带原图坐标的结果，前两份是 1×1 的颜色表，后两份是 3×2 的颜色表。大结果可以像单个矩阵一样用 `--out` 存成文件。

### 选择批量修改的写法

| 想画什么 | 修改单字段 | 每一项怎么写 |
|---|---|---|
| 零散的同色细节 | `points` | 一组或多组 `[x,y]`，共用这一项的 `color` |
| 同色衣纹或色块 | `rects` | 一组或多组 `[x,y,宽,高]`，共用这一项的 `color` |
| 自己设计的配色花纹 | `at + rows + palette` | 等宽字符串，点号跳过，擦除符由你定义 |
| 已知逐格颜色的矩形 | `at + pixels` | 二维 RGBA 数组，`null` 跳过，`"transparent"` 擦除 |

同一份修改单可把四种写法放进 `ops`，一次交给 `edit` 改 PNG，或交给 `edit-matrix` 改导出的单张颜色表。

### 用字符画图，一次交给 edit

假设想在小猫额头画一个小花纹，同时把两只眼睛换色。建一份修改单，按顺序列出操作。下面这段可以直接在 PowerShell 中运行。

~~~powershell
New-Item -ItemType Directory -Force '.\work' | Out-Null
@'
{
  "palette": {"A":"#78AEAD", "B":"#C8E9DA", "_":"transparent"},
  "ops": [
    {"points":[[4,5],[9,5]], "color":"#3877AA"},
    {"rects":[[5,3,2,1],[8,3,2,1]], "color":"#78AEAD"},
    {"at":[5,2], "rows":[".AB.","B.AB"]},
    {"at":[5,4], "pixels":[[null,"#C8E9DA","#78AEAD"]]},
    {"at":[2,0], "rows":["_."]}
  ]
}
'@ | Set-Content -Encoding utf8 '.\work\mark.json'
& $px edit --source $cat --scale 8 --patch '.\work\mark.json' --edited-source '.\work\cat-mark.png'
~~~

**怎样读这张修改单？** `points` 给多个点涂同一种颜色；`rects` 给多个矩形填同一种颜色。例如 `[5,3,2,1]` 从 (5,3) 开始，覆盖两格宽、一格高。把另一组颜色放进下一项操作，就能在一次调用中给多组位置换色。

`at` 指定网格左上角的原图坐标。`rows` 的一行字符串对应一行像素，各行要等宽。例子里 `A` 和 `B` 取自上方的颜色表。**`.` 保留这一格，`_` 擦掉这一格**，因为这里把 `_` 映射到了 `transparent`。下一次画同一个花纹时，只改颜色表就能试另一套配色。

如果已经知道每格的 RGBA，用 `pixels` 直接写二维颜色表。`null` 相当于字符网格里的点号，`"transparent"` 写入完全透明黑色。两项修改落在同一格时，按修改单顺序，后写的颜色生效。尺寸越界、未定义字符或不等宽的行会让整批报错，可以修正后重试。输出中的 `pixel_writes` 统计落笔次数，重复落在同一个位置会分别计数。

### edit-matrix · 先改颜色表，再把颜色表贴回原图

要先看数据再改图，可以把刚才的操作应用到单独的矩阵文件。继续在同一个 PowerShell 中运行：

~~~powershell
& $px matrix --source $cat --region '2,0,9,6' --out '.\work\cat-region.json'
& $px edit-matrix --source '.\work\cat-region.json' --patch '.\work\mark.json' --out '.\work\cat-region-marked.json'
& $px edit --source $cat --scale 8 --patch '.\work\cat-region-marked.json' --edited-source '.\work\cat-from-table.png'
~~~

第二行只保存一张新的颜色表。虽然选区起点是 (2,0)，修改单里的 (4,5) 依旧是原图的 (4,5)。第三行才会保存新的 PNG，并把整个选区（包括透明格）写回原位置。直接改 PNG 与先改矩阵再回填，使用同一份原图时，这个例子会得到逐像素相同的两只小猫。示例使用新文件名；重复实验时给输出另取名称。

## 四、选中已有像素再移动或复制

假设你想把小猫的两只眼睛一起往上挪一格。选中两只眼睛，就可以保留中间的鼻梁。用另存的候选图试落点，打开命令返回的 `preview.path` 看整张脸，或打开 `change_preview.path` 看改动处。

~~~powershell
& $px edit --source $cat --scale 8 --point '4,5' --point '9,5' --move-by '0,-1' --edited-source '.\work\cat-eyes-up.png'
~~~

`--move-by` 的两个数分别是横向和纵向的位移。一格向右写 `1,0`，一格向左写 `-1,0`，一格向下写 `0,1`。移走的格子会变透明，若要补上底下的毛色，可以再用设色命令填回。

想保留原处的像素，可先把选区存成剪贴文件。下面把两个单点和两条短色带当作一组，再将它们粘到同一画布的新位置。在手册根目录运行，每次练习用新的剪贴和成品文件名。

~~~powershell
New-Item -ItemType Directory -Force '.\work' | Out-Null
& $px copy --source $cat --point '4,5' --point '9,5' --rect '3,3,2,1' --rect '8,3,2,1' --out '.\work\face.clip.json'
& $px edit --source $cat --scale 8 --paste '.\work\face.clip.json' --at '2,4' --edited-source '.\work\cat-paste.png'
~~~

选区最左是 x=3，最上是 y=3。把基准坐标设为 (2,4)，原来的眼睛 (4,5) 就落到 (3,6)，另一只从 (9,5) 落到 (8,6)。两只眼睛之间未选中的格子会保留目标图上的颜色。若主动选入透明格，粘贴时它也会把对应目标格变透明。下次把 `--source` 换成另一张 PNG，同一个剪贴文件仍然能用。

要左右镜像，在移动或粘贴命令末尾加 `--flip-x`。要上下镜像，用 `--flip-y`。镜像会翻转整组选区，包括各块之间的相对位置。若目标超出画布，工具会报错，可调整基准坐标后重试。

## 五、同时打开素材和地图

如果要连续摆放多盏路灯，可以给素材和地图各开一张画布。每个名称背后有一个常驻进程，地图只在打开时解码一次。下面用随附的小猫当作素材，另用一份小猫图练习落点。在手册根目录运行。练习生成的剪贴文件和预览要用新的文件名。

~~~powershell
$px = '.\scripts\pixel-review.exe'
New-Item -ItemType Directory -Force '.\work' | Out-Null
& $px canvas open --canvas '素材' --source $cat
& $px canvas open --canvas '摆放练习' --source $cat
& $px canvas copy --canvas '素材' --rect '4,5,2,1' --clip '.\work\眼睛.clip.json'
& $px canvas close --canvas '素材'
~~~

现在素材进程已经退出。剪贴文件里保留了两格的颜色和相对位置，可以在另一张画布上连续试摆。两个坐标都用原图像素，向右、向下为正。

~~~powershell
& $px canvas paste --canvas '摆放练习' --clip '.\work\眼睛.clip.json' --at '3,6'
& $px canvas paste --canvas '摆放练习' --clip '.\work\眼睛.clip.json' --at '8,6'
& $px canvas preview --canvas '摆放练习' --region '2,4,10,4' --scale 12 --out '.\work\落点预览.png'
& $px canvas status --canvas '摆放练习'
~~~

打开预览可以看见两次粘贴的结果。此时普通图片查看器里打开的原 PNG 还是保存前的版本。也可以在同一条 paste 命令里重复 `--at`。贴树、人物等带透明背景的素材时，加 `--skip-transparent` 可让地面透过全透明的格子。

~~~powershell
& $px canvas save --canvas '摆放练习' --out '.\work\摆放成品.png'
& $px canvas close --canvas '摆放练习'
~~~

`save --out` 会将画布的保存路径切换到这个新文件。之后直接用 `save` 即可更新它。每次回执里的 `source.path` 告诉你当前会保存到哪里。若磁盘文件已被其他程序修改，写回会报错，可用 `--out` 保留两个版本。画布有改动时，普通 close 会提醒先保存；确定放弃这次改动才用 `close --discard`。常驻画布占用内存，退出或重启电脑前应保存需要保留的成品。

## 六、把可以独立修改的内容放进图层

想给小猫试一块脸部花纹，又想随时看到花纹下面的毛色，可以把小猫当底层，在上面加一层透明的小画布。下面从手册目录运行，用新的工程目录和输出文件名。

~~~powershell
$px = '.\scripts\pixel-review.exe'
New-Item -ItemType Directory -Force '.\work' | Out-Null
& $px canvas open --canvas 猫 --source $cat
& $px canvas layer-add --canvas 猫 --layer 花纹 --size '2,1' --at '4,5'
'{"palette":{"A":"#3877AA","B":"#80BBCC"},"ops":[{"at":[0,0],"rows":["AB"]}]}' | Set-Content -Encoding utf8 '.\work\花纹.json'
& $px canvas edit --canvas 猫 --layer 花纹 --patch '.\work\花纹.json'
& $px canvas preview --canvas 猫 --scale 12 --out '.\work\花纹原位置.png'
~~~

这里有两个原点。修改单里的 (0,0) 是花纹层的第一格，在整张小猫上显示在 (4,5)。要把花纹横向移动两格，只要把层的位置设成 (6,5)。原来被遮住的两格毛色会重新显现。已有角色 PNG 也可以用 `layer-add --layer 角色 --source 角色.png --at X,Y` 直接放到地图上。

~~~powershell
& $px canvas layer-set --canvas 猫 --layer 花纹 --at '6,5'
& $px canvas preview --canvas 猫 --scale 12 --out '.\work\花纹新位置.png'
& $px canvas save --canvas 猫 --project '.\work\小猫工程'
& $px canvas close --canvas 猫
& $px canvas open --canvas 猫 --source '.\work\小猫工程'
& $px canvas export --canvas 猫 --out '.\work\带花纹的小猫.png'
& $px canvas export --canvas 猫 --layer 花纹 --out '.\work\花纹素材.png'
& $px canvas close --canvas 猫
~~~

**保存工程**会保留每层的像素、位置、显示状态、层序和活动层。下次打开整个目录即可接着画，修改后运行 `canvas save --canvas 猫`。**导出合成图**会得到游戏可直接使用的整张 PNG。**导出单层**会得到原尺寸素材，这个例子里的花纹就是 2×1。

想观察底下的颜色，可以直接用 `canvas preview --canvas 猫 --layer Base --out 底层预览.png`。想看人物前面或后面放物件的区别，用 `layer-set --layer 物件 --index N` 调整层序；最底层是 0。打开状态下运行 `canvas status --canvas 猫` 随时可查层名与顺序。导出的 PNG 和放大预览都可以在手动保存工程前查看。

## 七、从零起稿、试错和统一配色

在手册目录打开 PowerShell。这次从一张宽 20、高 40 的透明 PNG 开始，头发和眼睛分两次提交，便于单独回退。每次练习使用新的底图和输出路径。

~~~powershell
$px = '.\scripts\pixel-review.exe'
& $px canvas create --canvas 草稿 --size '20,40' --out '.\work\底图.png'
'{"palette":{"H":"#BBCDEE","E":"#202536","_":"transparent"}}' | Set-Content -Encoding utf8 '.\work\角色色表.json'
& $px canvas palette-use --canvas 草稿 --palette '.\work\角色色表.json'
'{"ops":[{"at":[7,8],"rows":[".HHH.","HHHHH"]}]}' | Set-Content -Encoding utf8 '.\work\头发.json'
'{"ops":[{"at":[8,10],"rows":["E.E"]}]}' | Set-Content -Encoding utf8 '.\work\眼睛.json'
& $px canvas edit --canvas 草稿 --patch '.\work\头发.json'
& $px canvas edit --canvas 草稿 --patch '.\work\眼睛.json'
& $px canvas undo --canvas 草稿
& $px canvas preview --canvas 草稿 --region '5,6,10,8' --out '.\work\撤销眼睛.png'
& $px canvas redo --canvas 草稿
& $px canvas save --canvas 草稿 --project '.\work\角色工程'
& $px canvas export --canvas 草稿 --out '.\work\角色.png'
& $px canvas close --canvas 草稿
~~~

打开局部预览，你会看到上一次提交的头发，眼睛已回退。redo 会把眼睛恢复到同一位置。一次可以回退最近 32 笔，关闭再打开工程时会从已保存的画面开始一段新历史。只需要生成一张底图时可用 `create --size '20,40' --out 新图.png`。

**怎样统一已有素材里的近似色？** 假设已有正面和背面两张 PNG，色表规定头发用 H，背面可能误用了 `#BACDEE`。把路径换成你的素材，先运行检查，再按报告修正，最后对输出重新检查。

~~~powershell
$pal = '.\work\角色色表.json'
& $px palette-check --palette $pal --source 正面.png --source 背面.png
& $px recolor --palette $pal --source 正面.png --source 背面.png --map 'BACDEE=@H' --out-dir '.\work\统一配色'
& $px palette-check --palette $pal --source '.\work\统一配色\正面.png' --source '.\work\统一配色\背面.png'
~~~

报告中的 first 是该颜色第一次出现的原图坐标，count 是出现次数。核对特殊角色时可以加 `--allow '#DDBB77'` 临时允许一种服饰颜色。需要在工程中试色时，用 `canvas recolor --canvas 草稿 --layer Base --map 'BACDEE=@H'`，观察效果后可以 undo。磁盘批量换色会把结果另存到新目录。

## 八、看地砖铺满以后，墙角怎样接起来

在手册目录打开 PowerShell。下面在 work 中画两块 4×4 的砖，字母 A 代表地板、B 代表墙边。先观察单砖铺满后的接缝，再把两种砖拼成一间小屋。

~~~powershell
$px = '.\scripts\pixel-review.exe'
& $px create --size '8,4' --out '.\work\tiles.png'
'{"palette":{"X":"51565D","O":"ADB7B2","H":"C8AF92"},"ops":[{"at":[0,0],"rows":["XXXXHHHH","XOOOXXXX","XOOOXXXX","XOOOXXXX"]}]}' | Set-Content -Encoding utf8 '.\work\tiles-patch.json'
& $px edit --source '.\work\tiles.png' --patch '.\work\tiles-patch.json' --edited-source '.\work\atlas.png' --scale 1 --no-preview-changes
& $px tile-preview --source '.\work\atlas.png' --region '0,0,4,4' --repeat '5,5' --scale 4 --out '.\work\repeat.png'
'{"tiles":{"A":{"source":"atlas.png","region":[0,0,4,4]},"B":{"source":"atlas.png","region":[4,0,4,4]}},"rows":["BBBBBB","BAAAAB","BAAAAB","BAAAAB","BBBBBB"]}' | Set-Content -Encoding utf8 '.\work\room.json'
& $px tile-preview --layout '.\work\room.json' --scale 1 --out '.\work\room.png'
~~~

room.png 是宽 24、高 20 的原尺寸底图，可直接用于第十一节的组合评审。使用自己的直墙、内角和外角时，给每种砖分配字符，在布局中按它们实际相邻的顺序摆放。

## 九、看动画的节奏、重影与脚底

用随包的小猫做一个起跳练习。两帧的画布都是 14×12，第二帧的小猫比第一帧高 1 格。第三个播放位置重复第一帧，便于检查落回原地时的衔接。

~~~powershell
$px = '.\scripts\pixel-review.exe'
& $px resize --source '.\examples\cat.png' --top 2 --out '.\work\land.png'
& $px resize --source '.\examples\cat.png' --top 1 --bottom 1 --out '.\work\hop.png'
$walk = '.\work\hop.json'
& $px animation-config --frame '.\work\land.png' --frame '.\work\hop.png' --frame '.\work\land.png' --fps 6 --baseline 11 --out $walk
& $px animation --config $walk --scale 8 --out '.\work\hop.apng'
& $px onion --config $walk --index 2 --scale 8 --out '.\work\onion.png'
~~~

打开 hop.frames.png 可按编号比较三帧的位置。把 hop.apng 拖到浏览器标签页看循环速度，打开 onion.png 看第 2 帧与邻帧的叠色。配置保存帧序、帧率和基线，可在下次修改猫的姿势后复用。金线在第 11 行的下沿，第 2 帧的 bottom_delta 为 −1。叠色过浓用 `--opacity 25`，临时隐藏基线用 `--no-baseline`，只要播放文件时用 `--no-strip`。

## 十、给伸手动作留空间，保持鞋底对齐

假设角色原图宽 20、高 40，鞋底在第 37 行。为伸向右边的手多留 4 格，只扩右边就能得到 24×40 的画布，原有像素的坐标保持原值。以下命令使用你自己的 20×40 角色，将路径换成实际文件。

~~~powershell
$px = '.\scripts\pixel-review.exe'
& $px resize --source '.\work\角色原图.png' --right 4 --baseline 37 --out '.\work\伸手.png'
& $px resize --source '.\work\伸手.png' --crop '0,0,20,40' --out '.\work\裁回.png'
~~~

四个方向可独立组合。用 `--left 3 --top 2` 时，原像素 (5,37) 会移到 (8,39)，脚与身体仍然对齐。若希望脚底始终在第 37 行，可以只改左右或底边。裁掉顶部两行用 `--top -2`。JSON 回执里的 new_baseline 可直接填入新动画配置。对一组动画采用相同边距后再重新预览。

已有分层画布时，例如画布名为「场景」、手臂画在「角色」层，可以一起扩出可画的空间。

~~~powershell
& $px canvas resize --canvas 场景 --right 4 --pad-layer 角色 --baseline 37
& $px canvas preview --canvas 场景 --out '.\work\扩边预览.png'
~~~

此时可以在角色层的新四列继续画，其他图层仍与角色对齐。只想扩大取景范围时可省略 `--pad-layer`。裁小工程画布时，框外的图层内容会保存在工程里，之后把边界扩回来还能看到。普通 PNG 另存的裁切结果只含框内像素。试错用 `canvas undo --canvas 场景`，满意后用 `canvas save --canvas 场景` 保存已打开的工程。

## 十一、怎样让 AI 按同一套画法出稿、给你看图

第一次请 AI 为一个游戏起稿时，可以告诉它参考资料放在哪里。已有图库就直接给路径，新建时 skill 会询问位置，并把「外部参考」与「本项目已确认」分开放。你可以说「这版只定发型，衣服继续试」，AI 会把认可范围连同原图记下来。项目规范还可以记窗口倍率、尺寸、用色和场景取景。[美术规范模板](assets/art-guidelines.md)

起稿先看轮廓和主要色块，能在游戏大小下认出姿态，再加五官或材质。如果造型仍有两个方向，AI 可用一张评审图让你直接说「保留第 1 列的剪影，把第 2 列的发饰带过来」。细节改完后，用同样的设置和上一版并排复看。

用第八节的房间和第九节的起跳动作一起评审。每一列都把小猫放在房间的 (5,4)，可以比较落脚高度和背景对比。

~~~powershell
$px = '.\scripts\pixel-review.exe'
& $px review-sheet --before '.\work\land.png' --source '.\work\hop.png' --display-scale 2 --region '0,6,14,6' --detail-scale 10 --scene '.\work\room.png' --at '5,4' --out '.\work\hop-review.png'
~~~

打开 hop-review.png。上排看完整姿势，中排看放大的腿脚，下排看同一场景中的效果。第 1 列是旧版。比较多个候选时重复 `--source`；窗口倍率按你的游戏设置。[参数和坐标说明](cli.md#方案评审图)

### 扩边后，比较站位与遮挡

旧版 20×40 向右扩成 24×40 后，可直接传给 review-sheet。工具在共同画布中对齐两张图，右边腾出的空间可用来画伸出的手。不同尺寸默认按最低可见行对齐；如果衣摆、武器或影子低过靴底，分别指定每张原图里的同一只脚。例如向左扩 4 格后，旧图的 (9,37) 对应新图的 (13,37)，依次传 `--anchor 9,37 --anchor 13,37`。多个锚点按输出列排序，有 before 时从旧图开始。

~~~powershell
& $px review-sheet --before 旧版.png --source 伸手.png --anchor '9,37' --scene 房间.png --foreground 栏杆.png --at '149,102' --display-scale 2 --out 伸手评审.png
~~~

320×240 底图在这条命令下显示为 640×480，独立展示和场景中的角色大小一致。只想让场景以一倍显示时加 `--scene-scale 1`。如果房间已保存成分层工程，可把 `--scene 房间.png --foreground 栏杆.png` 换成 `--scene 房间工程 --replace-layer 角色`。省略 at 即沿用工程中角色的位置。栏杆、桌子和门框只要位于角色上方的可见图层，就会参与遮挡。工程和输入 PNG 都保留原样。

## 十二、帮助与许可

~~~powershell
& $px --help
& $px licenses --out '.\work\third-party-notices.zip'
~~~

`licenses` 将 EXE 内附的第三方许可写到一个新的 ZIP 文件。自动化调用可查看 `$LASTEXITCODE`。0 代表成功，1 表示 compare 找到差异，2 表示参数、文件或读写出错，原因可在 JSON 的 error 字段中找到。[完整参数与返回字段](cli.md)
