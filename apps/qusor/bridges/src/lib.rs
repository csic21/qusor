//! Host 与 Core 之间的可信适配器。
//!
//! 本 crate 只承载 Host 侧受信协调代码到 Core 用例之间的映射层：把公开 wire
//! 请求转成 `learning-application` 调用，并在回包前完成预算裁剪、控制字符净化
//! 与错误码映射。它不授予任何权限、不解析定位锚点、不直接写入存储，也不依赖
//! 任何隔离验证实验的目录或 crate。
//!
//! - [`notes_bridge`]：`notes-query-v1` 笔记读取适配器，受 4 KiB wire 预算约束。
//! - [`orthogonal_bridge`]：`core-knowledge-v1`（Host API 0.8+）正交读写与捕获适配器。
//! - [`vocab_bridge`]：`vocab-study-v1` 词汇学习适配器，不绕过调度策略。
//! - [`component_edit`]：组件编辑的受信协调器，串行化应用访问并管理
//!   invocation、租约与结果生命周期。
//!
//! 迁移后只依赖现成生产 crate：
//! `core-api`、`extension-api`、`extension-host`、`learning-application`、
//! `learning-ports`。
pub mod component_edit;
pub mod library_bridge;
pub mod notes_bridge;
pub mod orthogonal_bridge;
pub mod vocab_bridge;
