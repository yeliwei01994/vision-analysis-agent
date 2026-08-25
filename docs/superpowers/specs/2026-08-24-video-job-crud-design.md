# 视频任务 CRUD 设计

## 目标

为现有 React 视频任务页面增加任务查询、编辑和删除能力，并将任务状态可靠地持久化到 MySQL。

## 约束与规则

- 仅允许编辑任务显示文件名。
- 状态、进度、时长、物理路径和任务 ID 由系统控制。
- 删除采用软删除：任务写入 `deleted_at`，关联事件删除，物理视频文件保留。
- `queued` 或 `processing` 状态的任务删除返回 HTTP 409。
- 已删除任务不出现在任务列表、任务详情和事件列表中。

## 接口

- `GET /api/v1/jobs`：返回未软删除任务。
- `GET /api/v1/jobs/{id}`：返回未软删除任务。
- `PUT /api/v1/jobs/{id}`：请求体 `{ "filename": string }`，返回更新后的任务。
- `DELETE /api/v1/jobs/{id}`：删除允许状态的任务并返回 204。

## 数据流

React `JobsPage` 通过 API client 调用编辑/删除接口；Axum handler 校验输入并调用 `Database`；MySQL 迁移为 `video_jobs` 增加可空 `deleted_at` 与索引。删除在事务中更新任务、删除关联事件，且不触碰 `source_uri` 指向的文件。

## 错误处理

- 空文件名或超过 255 个字符返回 400。
- 不存在任务返回 404。
- queued/processing 删除返回 409。
- 数据库失败返回 500，前端显示错误并保留当前列表。

## 测试

- Rust API 合同测试覆盖编辑成功、输入校验、不存在任务、处理中任务拒删。
- 持久化测试覆盖软删除过滤和关联事件删除行为。
- React 测试覆盖编辑提交、删除确认和错误提示。
- 运行 `cargo test`、`npm test -- --run`、`npm run build` 与 `docker compose config`。
