# 本轮 NSH 设计依据核对

核对日期：2026-11-08。通过官方GitHub只读连接核对发布记录、近期提交、README及完整Design Guides。没有安装新技能、升级checkout或重跑平台能力；本记录不改变既有实验成绩。

| 对象 | 本轮观察 | 对当前设计的影响 |
|---|---|---|
| [发布记录](https://github.com/longbridge/nsh/releases) | 已查看v0.4.0（2026-11-04）及nightly（2026-11-08，目标f81b965f69096f871ca21907f6fab6be32fd2d40） | 固定实验仍需独立验证，发布不等于QUSOR已通过 |
| [main核对提交](https://github.com/longbridge/nsh/commit/9a6ae4e0b8611b8656433d05c4d8c0fe81407d28) | 2026-11-08，指针事件区域更新优化 | 分区/复杂编辑器需继续验证输入与渲染；不引用上游性能数字作为本项目成绩 |
| [前一提交](https://github.com/longbridge/nsh/commit/1042a80d876adcc3b9f86e75ebdf52bef4352083) | 性能监视器增加JS堆/节点观察 | 后续可验证真实资源增长，不能替代释放确认 |
| [固定快照README](https://github.com/longbridge/nsh/blob/9a6ae4e0b8611b8656433d05c4d8c0fe81407d28/README.md) | 区分原生行为基础与应用自有视觉层 | 统一token与样式组件后再组合工作区，不能以基础控件已有行为推定视觉系统自动完成 |
| [Design Guides](https://github.com/longbridge/nsh/blob/9a6ae4e0b8611b8656433d05c4d8c0fe81407d28/skills/nsh-design-guides/references/design-guides.md) | 已读设计系统、分区、状态、主题、输入、文案及评审部分 | 采用语义token、平台行为、明确滚动/焦点所有权与状态验收；本地命令使用按钮/导航组件，外链才使用链接语义 |

QUSOR当前运行实验仍固定 `913fd76e1f8434ec6cdf248f4476c1ebe08af19f`。新文档中的组件组合能力必须在该固定版本或后续隔离升级中实际核对，不从网页范例猜测NSH属性。Web/TUI不因上游支持而自动进入本项目交付范围。

同日开始原生工作区落地前，再次核对官方main、发布列表和完整Design Guides，main仍为上述9a6ae4e0提交，stable/nightly记录未变。本轮继续使用原固定checkout，不将上游文档示例的API存在性当作当前运行成绩。

本轮继续验证前再次通过官方连接核对：main已前进至 [1662115e30b4a86ca2e5eaf216e165c93bbdbee9](https://github.com/longbridge/nsh/commit/1662115e30b4a86ca2e5eaf216e165c93bbdbee9)（2026-11-08 06:57:15Z），新增按页释放节点ID表，涉及core、JS及桌面文件拖放。已核对该提交的Design Guides：区域滚动、可见焦点、覆盖层关闭及焦点恢复仍是设计要求。发布列表仍为v0.4.0与原nightly目标；当前资格保持913fd76e固定checkout，不临时cherry-pick。新内存修复值得后续隔离升级与长时节点增长测试，不能由上游测试替代本机native释放ack或据此宣称当前泄漏已解决。

公开组件契约接线本轮再次核对官方提交/发布及该快照Design Guides：main为 [893fb84db625045f36363ae9de1f999f588d25fd](https://github.com/longbridge/nsh/commit/893fb84db625045f36363ae9de1f999f588d25fd)（2026-11-08 07:19:09Z），编辑器打开时更早显示语法颜色；stable/nightly目标未变。规范仍要求原生行为基础、应用语义token、焦点与状态一致性。当前0.3 wire/Host不依赖NSH内部类型，不把该改进当输入法、富文本或可替换提供者已完成；保留913fd76e运行资格，不随协议接线升级实验checkout。
