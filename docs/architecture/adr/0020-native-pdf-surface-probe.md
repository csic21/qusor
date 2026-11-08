# ADR 0020：原生 PDF Surface 与选区适配实验

日期：2026-11-08。状态：本机原生Surface子项通过，非生产集成。关联 C-08/C-09、F-READ-01/03、F-SRC-01、P03/P05/P07/P08。

## 决定

在 NSH 独立实验 Workspace 增加 PDF Surface 探针，不把 NSH 或 PDFium 加入生产 Core。准备进程复用已资格的原生 PDF provider，生成真实页图和冻结字符映射；NSH 原生 Image 显示页图，原生 Canvas 绘制轻量选区叠层。没有 WebView/DOM，也不把页面位图当作已完成文字选择。

Rust 原生模块持有冻结报告、版本绑定、视口世代与 PointerSession；TSX 只传实际局部 DIP 事件和原生发放的手势 token。命中、范围、分段及失效规则复用已有 provider，不复制另一套近似命中。换页、缩放、窗口尺寸改变取消旧选区，空白或不支持的范围不默默吸附到近似文本。主线程启动与JS运行线程通过仅含中性数据的Mutex共享本轮单实例状态，不存NSH/PDFium对象；生产仍须受控Surface handle而非全局单实例。此轮输入仅桌面单鼠标，不能推导移动多指或系统级 pointer capture。

显示资源由同一轮原生 PPM 转 PNG，保留原始像素尺寸与摘要链。构建生成固定报告字节 SHA，原生启动报告必须匹配 bundle 内的绑定后才发放会话，防止旧页图与新报告混用。PNG只是显示编码转换，PDF/映射权威数据不从屏幕截图或 Canvas 反推。

阅读页、页码/倍率与候选引文分开；候选明确“尚未保存”，不提供未接持久化的假保存按钮。窄窗口使用原生滚动布局，滚动或出界期间取消活动手势。诊断 hash/token 留日志。

## 边界和验证

该模块是受控原生服务的技术探针，不是正式 reader 工作流、公开插件协议或生产安装包。正式 reader/library/notes 仍须作为普通独立功能插件通过公开 SDK 消费服务，不能凭实验允许第一方私有直连。

本轮分别核对原生构建/链接、真实窗口显示、实际事件、服务结果、截图与生命周期；最终状态以 [局部实验记录](../../../experiments/nsh-reader/pdf-surface/README.md) 为准。输入法、完整持久保存、业务恢复、EPUB、平台发布及性能均独立验收。


## 本机证据

[本机结果](../../../experiments/nsh-reader/evidence/linux-native-pdf-surface.json) 实际验证60%第一页raw拖动QUSOR、70%旋转页拖动Selection、空白/出界取消、缩放/换页/600px窄窗及正常关闭。原生适配3项边界测试和Clippy通过。旧PressUp提前终止、thread_local跨线程不可见和锁漂移拒绝日志保留，未据此降低原要求。正式插件与持久旅程仍未完成。
