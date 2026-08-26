# 视频处理性能优化实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 降低上传视频期间的 GET 轮询、列表查询、上传内存占用、任务状态写库频率和视频分析耗时，同时保持任务状态与事件结果正确。

**Architecture:** 先优化前端任务状态查询，再优化事件接口和上传存储，最后优化 Worker 的进度持久化与视频推理流水线。每个阶段独立可验证，先保留现有 API 兼容性，再逐步引入分页、增量查询和流式处理。

**Tech Stack:** React + TypeScript + Vitest；Rust + Axum + Tokio；MySQL/SQLx；Redis；FFmpeg；YOLO 推理服务。

**Spec:** 本计划对应用户提出的 1～6 项性能优化需求。

## Global Constraints

- 不改变任务状态语义：`pending`、`processing`、`completed`、`failed`、`cancelled`。
- 不删除现有 API；新增参数必须保持旧调用可用。
- 每项优化先补测试，再改实现，最后运行前端和后端测试。
- 前端源码位于 `frontend/src`，后端源码位于 `backend/src`。
- 使用真实视频、长任务和并发上传做最终性能验证。

### Task 1: 将任务轮询改为自适应、可取消的状态查询

**Files:**
- Modify: `frontend/src/App.tsx:67-74`
- Modify: `frontend/src/api/client.ts:19`
- Test: `frontend/src/App.test.tsx`

**Interfaces:**
- Produces: `waitForJob(id: string, signal?: AbortSignal): Promise<VideoJob>`；终态立即返回，超时抛出错误，请求可取消。

- [x] 新增测试：模拟 `processing → completed`，断言不会固定每秒创建无限请求，且完成后停止。
- [ ] 新增测试：卸载/取消时，`AbortController` 能终止当前请求。
- [x] 将固定 `setTimeout(..., 1000)` 改成递增间隔，例如 1 秒、2 秒、3 秒、5 秒、5 秒，最大 5 秒。
- [x] 每次请求完成后再等待下一次，禁止并发请求。
- [x] 任务进入 `completed`、`failed`、`cancelled` 后立即退出。
- [x] 前端测试：`npm test -- --run`；构建测试：`npm run build`。

### Task 2: 事件列表改为分页和增量查询

**Files:**
- Modify: `backend/src/api.rs`
- Modify: `backend/src/persistence.rs`
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/App.tsx`
- Test: `backend/tests/api_contract.rs`
- Test: `frontend/src/App.test.tsx`

**Interfaces:**
- Produces: `GET /api/v1/events/query?page=1&page_size=20&updated_after=<timestamp>`；旧的 `GET /api/v1/events` 保持可用。

- [ ] 新增后端测试：分页返回 `items`、`total`、`page`、`page_size`。
- [ ] 新增后端测试：`updated_after` 只返回变更事件。
- [ ] SQL 查询增加稳定排序和分页，避免一次加载全部事件。
- [ ] 前端列表首次只加载第一页，任务完成后只请求新增或变更事件。
- [ ] 详情页需要时再加载完整证据、分析字段，列表响应只返回摘要。
- [ ] 验证事件数量为 200、2000 时响应大小和耗时不会线性扩大到不可接受。

### Task 3: 上传改为流式落盘并限制并发

**Files:**
- Modify: `backend/src/api.rs:135-178`
- Modify: `backend/src/storage.rs`
- Modify: `backend/src/application.rs`
- Test: `backend/tests/upload_contract.rs`

**Interfaces:**
- Produces: 上传请求体按流写入临时文件，成功后原子移动到媒体目录；失败时删除临时文件。

- [ ] 新增测试：上传文件成功后文件内容与请求体一致。
- [ ] 新增测试：上传失败不会留下临时文件。
- [ ] 将“读取完整 bytes 后保存”改为 multipart 流式写入临时文件。
- [ ] 校验文件名、扩展名、大小上限和路径安全性。
- [ ] 增加上传并发信号量，超过并发数返回明确错误或排队。
- [ ] 使用大于 100 MB 的测试文件验证 API 内存不会随文件大小等比例增长。

### Task 4: 降低 Worker 进度写库频率（无需改造）

**Files:**
- Modify: `backend/src/worker.rs`
- Modify: `backend/src/application.rs`
- Modify: `backend/src/persistence.rs`
- Test: `backend/tests/worker_contract.rs`

**结论：** 当前 Worker 只在处理阶段边界和最终状态调用 `update_job`，没有逐帧写库行为；本项保留现状，避免引入无收益改动。

**Interfaces:**
- Produces: `ProgressReporter`，按时间间隔或进度变化阈值持久化任务状态；最终状态始终强制写库。

- [ ] 新增测试：连续进度更新不会每次都调用数据库。
- [ ] 新增测试：任务完成、失败时即使未达到时间间隔也会写入最终状态。
- [ ] 在 Worker 中设置最小写库间隔 1～2 秒或进度变化阈值 1%。
- [ ] 实时进度保留在内存/Redis，MySQL 保存关键状态和最终进度。
- [ ] 检查 Redis/API 重启后任务状态仍能从 MySQL 恢复。
- [ ] 运行：`cargo test --manifest-path backend/Cargo.toml`。

### Task 5: 视频抽帧和推理流水线优化

**Files:**
- Modify: `backend/src/video.rs`
- Modify: `backend/src/worker.rs`
- Modify: `backend/src/yolo.rs`
- Modify: `backend/tests/worker_contract.rs`

**Interfaces:**
- Produces: 可配置的检测抽帧间隔、批量推理能力和候选片段二次分析策略。

- [ ] 新增测试：给定视频时长和抽帧间隔，生成预期数量的检测帧。
- [ ] 新增测试：空检测结果不会触发事件或无意义的大模型调用。
- [ ] 将检测间隔、分辨率和批大小移入环境配置，保留当前默认值作为兼容配置。
- [ ] 优先使用低分辨率或固定最大边长抽帧。
- [ ] YOLO 服务支持批量帧请求；不支持时保留单帧回退路径。
- [ ] 只对检测到目标或满足规则候选的片段生成详细证据和后续分析。
- [ ] 用同一视频对比总处理时间、抽帧数量、YOLO 调用次数和事件数量。

### Task 6: 前端上传体验与任务生命周期解耦

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/api/client.ts`
- Test: `frontend/src/App.test.tsx`

**Interfaces:**
- Produces: 上传完成后立即创建任务并返回页面；任务状态在后台更新；用户可继续浏览事件和任务列表。

- [ ] 新增测试：上传完成后按钮不再持续等待整个视频分析过程。
- [ ] 新增测试：后台任务完成后刷新事件和任务列表一次。
- [ ] 将 `loading` 拆成 `uploading` 与 `processingJob` 两个状态。
- [ ] 上传完成后立即展示任务记录和初始状态，后台调用 `processVideo` 与 `waitForJob`。
- [ ] 页面卸载、重新上传或切换任务时取消旧任务的状态查询。
- [ ] 任务完成后只执行一次 `refreshEvents` 和 `listJobs`，避免重复完整查询。
- [ ] 运行：`npm test -- --run` 与 `npm run build`。

## 验收指标

- 单个视频处理期间的任务状态 GET 请求减少至少 60%。
- 任务完成后不再产生 `/api/v1/jobs/{id}` 请求。
- 事件列表首次响应体控制在 20 KB 以内，详情数据按需加载。
- 100 MB 以上视频上传时 API 内存不随文件大小完整复制。
- Worker 进度写库次数减少至少 80%，最终状态无丢失。
- 相同视频的推理调用次数和处理时长有可量化对比记录。
- 前端和后端现有测试全部通过。

## 执行顺序

1. Task 1：轮询优化，收益最快、风险最低。
2. Task 6：上传与处理解耦，改善用户体验。
3. Task 2：事件分页和增量查询，减少响应体。
4. Task 4：降低 Worker 写库频率。
5. Task 3：流式上传，解决大文件内存问题。
6. Task 5：抽帧和推理优化，重点改善真实处理耗时。
