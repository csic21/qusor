# 原生软件作品研究：从作者作品到 QUSOR

日期：2026-11-09。关联 C-09/C-11/C-12、F-UX-01/02 与 [通用基座](../design/agent-foundation.md)。用户指出已有稿偏 Web，要求研究优秀设计师的实际软件作品。此前机制研究不足以证明视觉研究充分；已有 HTML 稿降为结构与交互草案。

本轮查阅作者、团队和 Apple 的一手资料，并实际查看下列官方作品图片。截图只能支持可见的排版与控件观察；没有安装参考应用，不宣称实测过其手感、动画、性能或底层渲染实现。作者归属没有一手依据时只署团队，不猜个人姓名。

## 作品与可见证据

| 作者 / 团队 | 一手作品与出处 | 实际观察和适用范围 |
|---|---|---|
| Cultured Code 设计团队 | [Things](https://culturedcode.com/things/)、[功能与多窗口](https://culturedcode.com/things/features/)；[新版官方画面](https://static.culturedcode.com/things/videos/2026-06-25-meet-things-remastered-2/meettheallnewthings-poster-e.jpg) | 观察到窗口工具区、连续列表、清楚的行与图标对齐。新版图片是透视宣传裁切，不能据此量出真实字号。适合研究任务导航和日常工具的安静程度。 |
| Silvio Rizzi（应用作者） | [Reeder](https://reederapp.com/)、[Mela](https://mela.recipes/)；作者由 [Mela App Store 页面](https://apps.apple.com/fi/app/mela-recipe-manager/id1548466041) 与 [Reeder 页面](https://apps.apple.com/be/app/reeder/id6475002485)核对；[Reeder 官方界面](https://reederapp.com/reboot/screen.png)、[Mela 官方界面](https://mela.recipes/images/recipe.png) | Reeder 的导航、对象列表与内容分层明确；Mela 用正文与材料区组织阅读，较少容器装饰。借鉴内容与工具的关系。不能从截图推断 Reeder 所有内容都采用原生文字绘制。 |
| Sebastiaan de With / Lux 团队 | [Halide](https://halide.cam/)、[Kino](https://www.shotwithkino.com/)、[Kino 制作过程](https://www.lux.camera/kino-a-pro-video-camera-in-four-months/)；[Halide 官方控件画面](https://halide.cam/static/media/ui_portrait.jpg) | 制作过程明确展示 Sebastiaan 的早期设计。官方画面中的主要操作、次级信息和工具位置有清楚权重，控件具有器械感。适合研究精细控制，不把相机外观搬到通用工作台。 |
| Andy Allen / Andy Works，合作开发者 Mark Dawson | [!Boring](https://notbor.ing/)、[Apple 对 !Habits 的作者采访](https://developer.apple.com/news/?id=9ab1g4r3)；[采访中的作品画面](https://devimages-cdn.apple.com/wwdc-services/articles/images/5C19BE68-CEB3-4B29-B4BF-07E3D89991FB/2048.jpeg) | 实际查看了具立体感的完成反馈画面。采访介绍 SceneKit 与 UIKit/SwiftUI 的组合。它说明原生应用可以有独特视觉；QUSOR 只取局部反馈的精细程度，不采用全屏游戏化装饰。 |
| Panic 团队（补充桌面参照） | [Transmit](https://panic.com/transmit/)、[官方实界面](https://panic.com/transmit/images/screenshot1@2x.png) | 查看了双区工具、紧凑工具栏、文件列表及连接表单。它适合说明专业桌面密度，但文件管理器结构不适合作为所有 Agent 场景的外壳。没有把整款作品归给某一位个人。 |

## 对 QUSOR 的判断

主要参照 Things 和 Reeder 的桌面秩序，内容排版参考 Mela；局部控件的细节参考 Kino；!Boring 作为反馈设计的补充。判断依据是 QUSOR 的持续工作、任务切换与内容处理，不是获奖数量或宣传页的视觉冲击。

QUSOR 的窗口首先是一个可持续工作的空间。应用工具区、任务导航、主内容和临时检查器应有不同的材质、密度与焦点权重。列表靠行距、对齐、分组和选择态建立层级，避免把每项能力都做成大圆角卡片。原生感还包含可编辑文字、系统窗口行为、焦点、菜单与输入；交通灯和模糊本身不能完成这些事情。

文稿可以拥有纸面感，流程可以铺开步骤，画布可以占满舞台，代码可以成为一类内容。插件改变适用的内容与工具，宿主保留任务位置和统一控件规则。不要让阅读器、聊天页或 IDE 中的任何一种结构固化为整个基座。

字体选型先服从真实平台的字形与度量。macOS 样例使用 NSH 系统字体和系统中文回退，没有导入一套网页英文字体再任由中文代偿。具体字族名称、缩放、读屏和动态字号仍需专项测量，不能从一张图宣布完成。

## 本轮落实

- [原生视觉方向](../design/native-art-direction.md)：字阶、材质、窗口与控件的候选规则。
- [可运行的 NSH 样例](../../experiments/nsh-reader/native-design/README.md)：复用原生宿主，不添加 Core 依赖、WebView 或业务后端。
- [实窗证据](../../experiments/nsh-reader/evidence/native-design/README.md)：实际显示、输入、标签、任务隔离、主题、菜单和扩展列表；保留失败尝试与未验证范围。

原生窗口比 HTML 更接近实际显示结果，但它仍是视觉候选。真实插件安装、提供者替换、Agent 执行、正式保存和恢复继续使用各自门槛。
