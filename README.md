# Vision Analysis Agent

基于 Rust、React、MySQL、Redis 和 Docker 构建的视频事件检索与视觉分析平台。

平台的目标是把视频中的目标检测结果转换为可检索、可复核、可追踪的业务事件，并为后续接入真实 YOLO 模型和大模型分析接口保留扩展边界。

## 当前架构

```text
React 前端
    ↓
Nginx 反向代理
    ↓
Rust API ───── MySQL
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
| Database | MySQL 8.4 | 保存视频任务、事件和检测结果 |
| Queue | Redis 7 | 处理异步分析任务 |

## 目录结构

```text
vision-analysis-agent/
├── backend/                 # Rust API、Worker 和领域逻辑
│   ├── src/
│   └── tests/
├── frontend/                # React 前端
│   └── src/
├── db/                      # MySQL 数据库迁移和初始化数据
│   └── migrations/
├── deploy/                  # Nginx 和容器部署配置
│   └── nginx.conf
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
2. Rust API 保存视频并创建任务。
3. 任务状态写入 MySQL，并发送到 Redis 队列。
4. Worker 消费任务并执行视频分析。
5. 检测结果经过规则引擎生成业务事件。
6. 事件结果保存到 MySQL。
7. 前端查询事件并展示证据、分析和审核状态。

## 当前 API

```text
GET  /health

POST /api/v1/videos
POST /api/v1/videos/upload
POST /api/v1/videos/{id}/process

GET  /api/v1/jobs/{id}

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
Root 密码：root_dev_password
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

- 将 MockDetector 替换为真实 YOLO 推理服务；
- 接入大模型事件解释和自然语言检索；
- 生成事件截图和视频证据片段；
- 将 Redis Stream 升级为消费组、ACK、重试和死信队列；
- 增加用户权限、审计日志和系统监控；
- 增加开发模式热更新配置；
- 增加并发任务和长视频压力测试。

