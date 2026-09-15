# Vision Analysis Agent

基于 Rust、React、PostgreSQL、Redis 和 Docker 构建的视频事件检索与视觉分析平台。

平台的目标是把视频中的目标检测结果转换为可检索、可复核、可追踪的业务事件，并为后续接入真实 YOLO 模型和大模型分析接口保留扩展边界。

## 当前架构

```text
React 前端
    ↓
Nginx 反向代理
    ↓
Rust API ───── PostgreSQL
    ↓
Redis 队列
    ↓
Rust Worker
    ↓
检测器 / 规则引擎 / 大模型分析
```

| 服务 | 技术 | 作用 |
|---|---|---|
| API | Rust + Axum | 视频上传、任务管理、事件查询和审核接口 |
| Worker | Rust | 异步消费视频分析任务 |
| Frontend | React + Vite | 视频任务和事件检索工作台 |
| Nginx | Nginx | 前端静态资源和 API 反向代理 |
| 数据库 | PostgreSQL 16 + JSONB | 保存视频任务、事件和结构化检测结果 |
| 队列 | Redis 7 | 处理异步分析任务 |

## 目录结构

```text
vision-analysis-agent/
├── backend/                 # Rust API、Worker 和领域逻辑
│   ├── src/
│   └── tests/
├── frontend/                # React 前端
│   └── src/
├── db/                      # PostgreSQL 数据库迁移和初始化数据
│   └── migrations/
├── deploy/                  # Nginx 和容器部署配置
│   └── nginx.conf
├── inference/               # YOLOv8n 推理服务
│   └── yolo-service/
├── scripts/                 # 本地检查和启动脚本
├── docs/                    # 架构、开发计划和日报
├── docker-compose.yml       # 本地完整服务编排
├── .env.example             # 平台环境变量示例
└── README.md
```

## 快速启动

进入项目目录：

```powershell
cd D:\vision-analysis-agent
```

启动完整服务：

```powershell
docker compose up -d
```

查看服务状态：

```powershell
docker compose ps
```

访问地址：

- 前端工作台：http://localhost:8088
- API 服务：http://localhost:8080
- 健康检查：http://localhost:8080/health

首次修改 Rust 或 React 代码后，需要重新构建对应镜像：

```powershell
docker compose build api worker frontend
docker compose up -d
```

## 当前业务流程

1. 用户通过 React 页面上传视频。
2. Rust API 保存视频、创建任务并自动发送到 Redis 队列。
3. Worker 消费任务，使用 ffmpeg 抽取 JPEG 帧。
4. Rust `YoloDetector` 调用 `YOLO_URL/v1/infer/frame` 获取真实检测结果。
5. 检测结果经过规则引擎生成业务事件，并保存模型版本。
6. 事件结果保存到 PostgreSQL；YOLO 检测对象、证据、规则几何和模型分析等结构化数据以 JSONB 保存。
7. 前端查询事件并展示证据、分析和审核状态。

## 当前 API

```text
GET  /health

POST /api/v1/videos
POST /api/v1/videos/upload
POST /api/v1/videos/{id}/process

GET  /api/v1/jobs/{id}
PUT  /api/v1/jobs/{id}
DELETE /api/v1/jobs/{id}

GET  /api/v1/events
GET  /api/v1/events/{id}
POST /api/v1/events/search
POST /api/v1/events/{id}/confirm
POST /api/v1/events/{id}/ignore

GET  /api/v1/event-rules
PUT  /api/v1/event-rules/{event_type}
```

## 数据库开发账号

当前 Docker Compose 开发环境配置为：

```text
数据库：vision_events
用户：vision
密码：vision_dev_password
```

这些密码仅用于本地开发，生产环境必须通过环境变量或密钥管理系统替换。

## 本地测试

Rust 后端：

```powershell
cd backend
cargo test
```

React 前端：

```powershell
cd frontend
npm install
npm test -- --run
npm run build
```

检查 Compose 配置：

```powershell
docker compose config
```

## 当前实现边界

当前平台已经完成基础任务链路、事件规则接口、前端事件审核和 Docker 部署，但仍有以下演进工作：

- 接入大模型事件解释和自然语言检索；
- 生成事件截图和视频证据片段；
- 完善 Redis Stream 的重试、死信队列和可观测性；
- 增加用户权限、审计日志和系统监控；
- 增加开发模式热更新配置；
- 增加并发任务和长视频压力测试。

## YOLOv8n 推理服务

当前已加入独立的 YOLOv8n CPU 推理服务，接口地址为 `http://localhost:9000`（Docker 内部为 `http://yolo:9000`）：

```text
GET  /health
POST /v1/infer/frame
```

启动服务：

```powershell
docker compose up -d yolo
```

模型权重会缓存到 Docker volume `yolo_models`。上传视频后会自动进入 Worker，前端事件详情显示真实目标类别、置信度和检测器模型版本。

### 检测能力与事件规则

YOLO 负责输出帧内的目标检测结果（例如 `person`、车辆等模型支持的类别）；它并不直接输出“停留”“闯入”等业务行为。Worker 将连续帧的目标结果交给事件规则引擎，再生成可检索、可审核的事件。

当前内置并默认启用的事件规则为 `person_stay`（人员停留）：当人员在规则设定的时间/区域条件内持续出现时，系统生成“人员停留”事件。因此，页面仅显示该事件类型是当前实现范围的正常表现，并不表示 YOLO 只能检测人员。

区域入侵、车辆停留、聚集、未戴安全帽等场景需要分别实现并启用对应的事件规则，同时配置适用的 YOLO 目标类别、区域和时间阈值。

完整 E2E 测试：

```powershell
.\scripts\e2e-yolo.ps1
```

## 性能基准

视频分析性能可以使用 [视频性能基准测试](docs/performance/video-benchmark.md) 对固定视频和不同 YOLO batch/并发配置进行量化，结果会输出为 JSON 和 CSV。
