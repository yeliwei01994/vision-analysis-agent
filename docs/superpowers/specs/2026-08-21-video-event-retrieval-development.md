# 视频事件检索平台开发实施文档

日期：2026-08-21  
状态：开发规划  
关联设计：[2026-08-21-vision-analysis-platform-design.md](./2026-08-21-vision-analysis-platform-design.md)

## 1. 文档定位

原设计文档定义了图片分析平台第一阶段：React、FastAPI、MySQL、Redis、Docker Compose 和 Nginx。本文件定义第二阶段的开发方向：在保留图片分析能力和兼容接口的基础上，逐步建设面向视频的事件生成、证据留存和自然语言检索能力。

本阶段先完成稳定的工程骨架和数据契约，不把具体检测类别、Prompt 或某个大模型写死在业务代码中。后续新增“未戴安全帽”“进入禁区”“人员滞留”等能力时，应通过模型配置、事件规则和 Prompt 版本完成扩展。

## 2. 产品目标

用户可以上传视频或接入视频文件，系统对视频进行分析并生成带时间戳的视觉事件。用户可以按摄像头、时间范围、事件类型、严重程度和关键词检索事件，并查看对应截图、短视频片段、检测结果和大模型解释。

核心流程：

```text
视频上传/导入
  → 抽帧与目标检测
  → 多目标跟踪
  → 规则引擎生成事件
  → 关键帧/片段留证
  → 大模型分析与摘要
  → MySQL 保存事件
  → React 检索、回放与人工复核
```

## 3. 本阶段范围

### 3.1 实现范围

- Rust API 服务和异步任务 Worker。
- React + Vite + TypeScript 前端。
- Nginx 静态托管和 API 反向代理。
- MySQL 保存任务、事件、检测结果和版本信息。
- Redis 保存任务队列、进度、短期缓存和去重状态。
- Docker Compose 本地和测试环境编排。
- 视频上传、文件管理、抽帧、检测、跟踪和事件生成接口。
- 大模型分析接口，第一阶段允许使用 Mock 实现。
- 事件列表、筛选、详情、证据回放和人工确认。

### 3.2 暂不实现

- 多租户、登录、权限和团队协作。
- Kafka、Kubernetes 和大规模分布式调度。
- 直接控制摄像头、门禁或生产设备。
- 高风险事件的自动处置。
- 第一阶段的 RTSP 长时间实时流分析。
- 强绑定某一个 YOLO 版本、云端大模型或向量数据库。

实时 RTSP、向量检索和告警通知作为接口扩展点保留，待离线视频链路稳定后再实现。

## 4. 技术架构

```text
Browser
  ↓
Nginx
  ├── /       → React 静态资源
  └── /api/   → Rust API
                  ├── 视频与任务管理
                  ├── 事件检索
                  ├── 模型/规则/Prompt 配置
                  └── Redis Streams
                         ↓
                      Rust Worker
                       ├── FFmpeg 抽帧与切片
                       ├── Detector Adapter → YOLO/ONNX/TensorRT
                       ├── Tracker Adapter
                       ├── Event Rule Engine
                       └── Vision Analyzer → 大模型/Mock
                              ↓
                    MySQL + 文件存储 + Redis
```

视频和图片二进制不存入 MySQL 或 Redis。开发阶段可使用本地 volume，生产阶段建议使用 MinIO 或兼容 S3 的对象存储；通过 `Storage` 抽象保持迁移成本可控。

## 5. Rust 服务边界

建议使用 Rust workspace：

```text
backend/
├── crates/
│   ├── api/           # Axum 路由、鉴权边界、OpenAPI
│   ├── domain/        # 任务、事件、检测、版本等领域模型
│   ├── application/   # 用例编排
│   ├── detector/      # YOLO/ONNX/TensorRT Adapter
│   ├── tracker/       # 跟踪器 Adapter
│   ├── event-engine/  # 规则和事件状态机
│   ├── analyzer/      # 大模型接口和 Mock 实现
│   ├── repository/    # MySQL、Redis 数据访问
│   ├── storage/       # 本地文件、MinIO/S3
│   └── worker/        # Redis Streams 消费和任务执行
└── migrations/
```

推荐技术选择：Axum、Tokio、SQLx、Serde、Redis crate、Tracing、Reqwest 和 utoipa。框架选择可以调整，但业务层不得直接依赖某个具体 YOLO SDK 或大模型 SDK。

### 5.1 检测接口

```rust
#[async_trait]
pub trait Detector: Send + Sync {
    async fn detect(&self, frame: Frame) -> Result<Vec<Detection>>;
}
```

检测结果至少包含：类别、置信度、边界框、帧号、时间戳和可选 `track_id`。模型名称、模型版本和推理耗时必须进入结果元数据。

### 5.2 大模型接口

```rust
#[async_trait]
pub trait VisionAnalyzer: Send + Sync {
    async fn analyze(&self, context: EventContext) -> Result<AnalysisResult>;
}
```

大模型只处理事件上下文，不处理全部视频帧。上下文可以包括关键帧、前后时间窗口、检测目标、轨迹摘要、摄像头位置、业务场景和相关 SOP。输出必须是结构化 JSON，并记录模型和 Prompt 版本。

## 6. 领域模型

### 6.1 视频任务

`video_jobs` 保存一次视频分析任务：

- `id`
- `source_type`：`upload`、`file`、未来的 `rtsp`
- `source_uri`
- `filename`
- `sha256`
- `duration_ms`
- `status`：`pending`、`processing`、`completed`、`failed`、`cancelled`
- `progress`
- `detector_version`
- `created_at`、`started_at`、`finished_at`
- `error_code`、`error_message`

### 6.2 视觉事件

`events` 是平台的核心查询对象：

```json
{
  "id": "event_123",
  "job_id": "job_001",
  "camera_id": null,
  "event_type": "person_enter_zone",
  "start_time_ms": 182000,
  "end_time_ms": 197000,
  "severity": "high",
  "status": "unreviewed",
  "confidence": 0.91,
  "objects": [
    { "class_name": "person", "track_id": 17 }
  ],
  "evidence": {
    "thumbnail_url": "/media/events/event_123.jpg",
    "clip_url": "/media/events/event_123.mp4",
    "frame_urls": []
  },
  "analysis": {},
  "rule_version": "zone-v1",
  "prompt_version": "event-v1",
  "created_at": "2026-08-21T10:20:00Z"
}
```

事件类型必须是可配置字符串，不能使用数据库枚举锁死。事件的原始检测结果和大模型分析结果可分别保存在 JSON 字段中，同时保留用于检索的结构化字段。

## 7. 事件引擎设计

第一阶段使用规则驱动，不依赖大模型决定是否产生事件。规则输入包括：

- 目标类别和属性。
- 区域多边形。
- 进入、离开和停留时长。
- 目标之间的距离。
- 连续帧数量和置信度阈值。
- 事件冷却时间。

规则输出事件候选，大模型负责解释、归因、摘要和建议。这样可以保证事件触发可测试、可复现，也避免大模型误判直接产生高风险动作。

规则配置示例：

```json
{
  "event_type": "person_enter_zone",
  "enabled": true,
  "conditions": {
    "class_name": "person",
    "zone_id": "restricted_area",
    "min_duration_ms": 10000,
    "min_confidence": 0.65
  },
  "cooldown_ms": 60000,
  "prompt_key": "event_analysis"
}
```

## 8. API 契约

### 8.1 视频和任务

```text
POST   /api/v1/videos
GET    /api/v1/videos/{id}
POST   /api/v1/videos/{id}/process
POST   /api/v1/videos/{id}/cancel
GET    /api/v1/jobs/{id}
```

### 8.2 事件检索

```text
GET    /api/v1/events
GET    /api/v1/events/{id}
PATCH  /api/v1/events/{id}/status
GET    /api/v1/events/{id}/evidence
POST   /api/v1/events/search
```

结构化检索请求：

```json
{
  "event_type": "person_enter_zone",
  "severity": ["high", "medium"],
  "status": "unreviewed",
  "start_time": "2026-08-20T00:00:00Z",
  "end_time": "2026-08-21T00:00:00Z",
  "keyword": "仓库入口",
  "page": 1,
  "page_size": 20
}
```

自然语言检索后续可以增加：

```text
POST /api/v1/events/search/natural-language
```

它只负责把自然语言转换为结构化查询条件，再由后端执行查询；不允许大模型直接拼接 SQL。

### 8.3 配置接口

```text
GET    /api/v1/event-types
POST   /api/v1/event-rules
GET    /api/v1/prompts
POST   /api/v1/prompts
GET    /api/v1/models
POST   /api/v1/models
```

Prompt、规则和模型都必须支持版本号、启用状态和创建时间。历史事件保存实际使用的版本，确保结果可审计。

## 9. 数据库与 Redis

MySQL 建议至少包含：

- `video_jobs`
- `events`
- `event_rules`
- `prompt_versions`
- `model_versions`
- `event_reviews`

`events` 对 `event_type`、`severity`、`status`、`start_time`、`created_at` 建索引；完整检测轨迹和大模型输出放 JSON。只有当查询需求稳定后，才把高频字段拆成独立关系表。

Redis 使用：

```text
vision:job:{id}:progress
vision:queue:video-analysis
vision:event-search:{query_hash}
vision:dedup:{job_id}:{event_fingerprint}
```

Redis 故障不能导致 MySQL 数据丢失。任务消费需要幂等：同一个视频任务或事件指纹重复执行时，不应生成无限重复事件。

## 10. 前端功能

React 按 feature 拆分：

```text
frontend/src/
├── api/
├── types/
├── components/
├── features/videos/
├── features/events/
├── features/search/
├── features/review/
└── features/settings/
```

第一版页面：

1. 视频上传和任务进度。
2. 事件列表和筛选。
3. 事件详情、截图和视频片段回放。
4. 视频时间轴，标出事件位置。
5. 人工确认、忽略和修改事件。
6. 事件类型、规则和 Prompt 配置。

检测框叠加组件继续采用原设计中的前端绘制方案。视频时间轴只依赖事件的起止时间，不依赖具体检测模型，因此未来切换模型不影响页面。

## 11. Docker 与 Nginx

第一阶段 Compose 服务：

- `api`
- `worker`
- `frontend`
- `mysql`
- `redis`
- `minio`（推荐；也可先使用本地 volume）

Nginx 负责：

- React SPA 静态资源和路由回退。
- `/api/` 代理到 Rust API。
- `/media/` 代理到文件存储或对象存储。
- 上传大小、API 超时和视频回放相关配置。

API、Worker、MySQL 和 Redis 不暴露到公网。所有密钥通过环境变量注入，不能进入 React 构建产物。

## 12. 分阶段开发计划

### 阶段一：工程骨架和契约

- 初始化 Rust workspace、React 工程和 Compose。
- 完成健康检查、配置加载、日志、错误响应和 OpenAPI。
- 建立 `VideoJob`、`Event`、`Detection`、`AnalysisResult` 类型。
- 实现 Mock Detector、Mock Analyzer 和内存任务流。
- React 完成任务列表、事件列表和详情页面的 Mock 联调。

验收：不接真实 YOLO 和大模型，也能完整演示“创建任务 → 生成事件 → 检索 → 查看详情”。

### 阶段二：视频文件链路

- 接入文件上传和存储。
- 使用 FFmpeg 获取视频元数据、抽帧和切片。
- 接入 YOLO Adapter。
- Redis Streams 驱动 Worker，持久化任务状态。

验收：上传一个视频后可以看到任务进度、检测结果和事件时间线。

### 阶段三：规则事件和证据

- 实现区域、停留时间、越界和冷却规则。
- 生成关键帧和事件短视频片段。
- 完成事件去重、失败重试和幂等。
- 接入 MySQL 事件检索。

验收：同一视频重复处理不会产生无限重复事件，事件可定位到具体时间段。

### 阶段四：大模型分析

- 实现统一 VisionAnalyzer 接口。
- 接入真实视觉大模型。
- Prompt 版本化和结构化输出校验。
- 增加报告、风险等级、建议和人工复核字段。

验收：大模型不可用时规则事件仍然生成，结果明确标记为 `fallback` 或 `analysis_pending`。

### 阶段五：自然语言检索和优化

- 自然语言转结构化过滤条件。
- 增加事件统计和日报。
- 根据人工复核结果优化阈值、规则和 Prompt。
- 评估是否需要向量索引或实时 RTSP。

## 13. 测试与可观测性

- 单元测试：规则判断、区域计算、事件合并、指纹去重和 Prompt Schema 校验。
- API 测试：上传、任务状态、分页检索、详情和人工复核。
- Worker 测试：重试、取消、幂等和 Redis 不可用降级。
- 集成测试：Compose 启动、MySQL 持久化、对象存储和视频回放。
- 前端测试：任务进度、筛选条件、时间轴和错误状态。

日志必须包含 `request_id`、`job_id`、`event_id`、模型版本和耗时。建议记录检测帧数、事件数量、误报复核数量、大模型调用次数、平均延迟和单任务成本。

## 14. 生产安全边界

- 大模型不能直接执行删除、停机、开门等高风险动作。
- 事件结论必须保存原始证据和模型版本。
- 低置信度事件进入人工复核队列。
- 视频访问 URL 应使用受控资源路径或短期签名 URL。
- 上传文件需要限制类型、大小和处理目录，防止路径穿越。
- Prompt 和规则修改需要保留版本，后续可增加操作审计。

## 15. 最终验收标准

- Rust API、Worker、React、MySQL、Redis 和文件存储可通过 Docker Compose 启动。
- 可以上传视频、创建任务、查看进度并生成事件。
- 事件包含时间范围、类型、置信度、证据和模型版本。
- 可以按结构化条件查询并打开事件短视频。
- YOLO 和大模型均通过接口抽象，允许 Mock、替换和版本升级。
- 大模型失败不阻断基础检测和事件生成。
- 规则、Prompt 和模型配置可以版本化。
- 任务执行具备重试、取消、幂等和失败可观测性。
- Nginx 可以托管 React、代理 API 并提供媒体访问。
- 原图片分析接口仍可兼容，或通过明确的迁移适配层保留。

